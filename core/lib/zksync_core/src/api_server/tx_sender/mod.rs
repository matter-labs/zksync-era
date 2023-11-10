//! Helper module to submit transactions into the zkSync Network.

// External uses
use governor::{
    clock::MonotonicClock,
    middleware::NoOpMiddleware,
    state::{InMemoryState, NotKeyed},
    Quota, RateLimiter,
};

// Built-in uses
use std::{cmp, num::NonZeroU32, sync::Arc, time::Instant};

// Workspace uses

use multivm::interface::VmExecutionResultAndLogs;
use multivm::vm_latest::{
    constants::{BLOCK_GAS_LIMIT, MAX_PUBDATA_PER_BLOCK},
    utils::{
        fee::derive_base_fee_and_gas_per_pubdata,
        overhead::{derive_overhead, OverheadCoeficients},
    },
};

use zksync_config::configs::{api::Web3JsonRpcConfig, chain::StateKeeperConfig};
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{transactions_dal::L2TxSubmissionResult, ConnectionPool};
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    fee::{Fee, TransactionExecutionMetrics},
    get_code_key, get_intrinsic_constants,
    l2::error::TxCheckError::TxDuplication,
    l2::L2Tx,
    utils::storage_key_for_eth_balance,
    AccountTreeId, Address, ExecuteTransactionCommon, L2ChainId, Nonce, PackedEthSignature,
    ProtocolVersionId, Transaction, H160, H256, MAX_GAS_PER_PUBDATA_BYTE, MAX_L2_TX_GAS_LIMIT,
    MAX_NEW_FACTORY_DEPS, U256,
};

use zksync_utils::h256_to_u256;

// Local uses
use crate::api_server::{
    execution_sandbox::{
        adjust_l1_gas_price_for_tx, execute_tx_eth_call, execute_tx_with_pending_state,
        get_pubdata_for_factory_deps, BlockArgs, SubmitTxStage, TxExecutionArgs, TxSharedArgs,
        VmConcurrencyLimiter, VmPermit, SANDBOX_METRICS,
    },
    tx_sender::result::ApiCallResult,
};
use crate::{
    l1_gas_price::L1GasPriceProvider,
    metrics::{TxStage, APP_METRICS},
    state_keeper::seal_criteria::{ConditionalSealer, SealData},
};

mod proxy;
mod result;

pub(super) use self::{proxy::TxProxy, result::SubmitTxError};

/// Type alias for the rate limiter implementation.
type TxSenderRateLimiter =
    RateLimiter<NotKeyed, InMemoryState, MonotonicClock, NoOpMiddleware<Instant>>;

#[derive(Debug, Clone)]
pub struct MultiVMBaseSystemContracts {
    /// Contracts to be used for pre-virtual-blocks protocol versions.
    pub(crate) pre_virtual_blocks: BaseSystemContracts,
    /// Contracts to be used for post-virtual-blocks protocol versions.
    pub(crate) post_virtual_blocks: BaseSystemContracts,
    /// Contracts to be used for protocol versions after virtual block upgrade fix.
    pub(crate) post_virtual_blocks_finish_upgrade_fix: BaseSystemContracts,
    /// Contracts to be used for post-boojum protocol versions.
    pub(crate) post_boojum: BaseSystemContracts,
}

impl MultiVMBaseSystemContracts {
    pub fn get_by_protocol_version(self, version: ProtocolVersionId) -> BaseSystemContracts {
        match version {
            ProtocolVersionId::Version0
            | ProtocolVersionId::Version1
            | ProtocolVersionId::Version2
            | ProtocolVersionId::Version3
            | ProtocolVersionId::Version4
            | ProtocolVersionId::Version5
            | ProtocolVersionId::Version6
            | ProtocolVersionId::Version7
            | ProtocolVersionId::Version8
            | ProtocolVersionId::Version9
            | ProtocolVersionId::Version10
            | ProtocolVersionId::Version11
            | ProtocolVersionId::Version12 => self.pre_virtual_blocks,
            ProtocolVersionId::Version13 => self.post_virtual_blocks,
            ProtocolVersionId::Version14
            | ProtocolVersionId::Version15
            | ProtocolVersionId::Version16
            | ProtocolVersionId::Version17 => self.post_virtual_blocks_finish_upgrade_fix,
            ProtocolVersionId::Version18 | ProtocolVersionId::Version19 => self.post_boojum,
        }
    }
}

/// Smart contracts to be used in the API sandbox requests, e.g. for estimating gas and
/// performing `eth_call` requests.
#[derive(Debug, Clone)]
pub struct ApiContracts {
    /// Contracts to be used when estimating gas.
    /// These contracts (mainly, bootloader) normally should be tuned to provide accurate
    /// execution metrics.
    pub(crate) estimate_gas: MultiVMBaseSystemContracts,
    /// Contracts to be used when performing `eth_call` requests.
    /// These contracts (mainly, bootloader) normally should be tuned to provide better UX
    /// exeprience (e.g. revert messages).
    pub(crate) eth_call: MultiVMBaseSystemContracts,
}

impl ApiContracts {
    /// Loads the contracts from the local file system.
    /// This method is *currently* preferred to be used in all contexts,
    /// given that there is no way to fetch "playground" contracts from the main node.
    pub fn load_from_disk() -> Self {
        Self {
            estimate_gas: MultiVMBaseSystemContracts {
                pre_virtual_blocks: BaseSystemContracts::estimate_gas_pre_virtual_blocks(),
                post_virtual_blocks: BaseSystemContracts::estimate_gas_post_virtual_blocks(),
                post_virtual_blocks_finish_upgrade_fix:
                    BaseSystemContracts::estimate_gas_post_virtual_blocks_finish_upgrade_fix(),
                post_boojum: BaseSystemContracts::estimate_gas_post_boojum(),
            },
            eth_call: MultiVMBaseSystemContracts {
                pre_virtual_blocks: BaseSystemContracts::playground_pre_virtual_blocks(),
                post_virtual_blocks: BaseSystemContracts::playground_post_virtual_blocks(),
                post_virtual_blocks_finish_upgrade_fix:
                    BaseSystemContracts::playground_post_virtual_blocks_finish_upgrade_fix(),
                post_boojum: BaseSystemContracts::playground_post_boojum(),
            },
        }
    }
}

/// Builder for the `TxSender`.
#[derive(Debug)]
pub struct TxSenderBuilder {
    /// Shared TxSender configuration.
    config: TxSenderConfig,
    /// Connection pool for read requests.
    replica_connection_pool: ConnectionPool,
    /// Connection pool for write requests. If not set, `proxy` must be set.
    master_connection_pool: Option<ConnectionPool>,
    /// Rate limiter for tx submissions.
    rate_limiter: Option<TxSenderRateLimiter>,
    /// Proxy to submit transactions to the network. If not set, `master_connection_pool` must be set.
    proxy: Option<TxProxy>,
    /// Actual state keeper configuration, required for tx verification.
    /// If not set, transactions would not be checked against seal criteria.
    state_keeper_config: Option<StateKeeperConfig>,
}

impl TxSenderBuilder {
    pub fn new(config: TxSenderConfig, replica_connection_pool: ConnectionPool) -> Self {
        Self {
            config,
            replica_connection_pool,
            master_connection_pool: None,
            rate_limiter: None,
            proxy: None,
            state_keeper_config: None,
        }
    }

    pub fn with_rate_limiter(self, transactions_per_sec: u32) -> Self {
        let rate_limiter = RateLimiter::direct_with_clock(
            Quota::per_second(NonZeroU32::new(transactions_per_sec).unwrap()),
            &MonotonicClock,
        );
        Self {
            rate_limiter: Some(rate_limiter),
            ..self
        }
    }

    pub fn with_tx_proxy(mut self, main_node_url: &str) -> Self {
        self.proxy = Some(TxProxy::new(main_node_url));
        self
    }

    pub fn with_main_connection_pool(mut self, master_connection_pool: ConnectionPool) -> Self {
        self.master_connection_pool = Some(master_connection_pool);
        self
    }

    pub fn with_state_keeper_config(mut self, state_keeper_config: StateKeeperConfig) -> Self {
        self.state_keeper_config = Some(state_keeper_config);
        self
    }

    pub async fn build<G: L1GasPriceProvider>(
        self,
        l1_gas_price_source: Arc<G>,
        vm_concurrency_limiter: Arc<VmConcurrencyLimiter>,
        api_contracts: ApiContracts,
        storage_caches: PostgresStorageCaches,
    ) -> TxSender<G> {
        assert!(
            self.master_connection_pool.is_some() || self.proxy.is_some(),
            "Either master connection pool or proxy must be set"
        );

        TxSender(Arc::new(TxSenderInner {
            sender_config: self.config,
            master_connection_pool: self.master_connection_pool,
            replica_connection_pool: self.replica_connection_pool,
            l1_gas_price_source,
            api_contracts,
            rate_limiter: self.rate_limiter,
            proxy: self.proxy,
            state_keeper_config: self.state_keeper_config,
            vm_concurrency_limiter,
            storage_caches,
        }))
    }
}

/// Internal static `TxSender` configuration.
/// This structure is detached from `ZkSyncConfig`, since different node types (main, external, etc)
/// may require different configuration layouts.
/// The intention is to only keep the actually used information here.
#[derive(Debug, Clone)]
pub struct TxSenderConfig {
    pub fee_account_addr: Address,
    pub gas_price_scale_factor: f64,
    pub max_nonce_ahead: u32,
    pub max_allowed_l2_tx_gas_limit: u32,
    pub fair_l2_gas_price: u64,
    pub vm_execution_cache_misses_limit: Option<usize>,
    pub validation_computational_gas_limit: u32,
    pub chain_id: L2ChainId,
}

impl TxSenderConfig {
    pub fn new(
        state_keeper_config: &StateKeeperConfig,
        web3_json_config: &Web3JsonRpcConfig,
        chain_id: L2ChainId,
    ) -> Self {
        Self {
            fee_account_addr: state_keeper_config.fee_account_addr,
            gas_price_scale_factor: web3_json_config.gas_price_scale_factor,
            max_nonce_ahead: web3_json_config.max_nonce_ahead,
            max_allowed_l2_tx_gas_limit: state_keeper_config.max_allowed_l2_tx_gas_limit,
            fair_l2_gas_price: state_keeper_config.fair_l2_gas_price,
            vm_execution_cache_misses_limit: web3_json_config.vm_execution_cache_misses_limit,
            validation_computational_gas_limit: state_keeper_config
                .validation_computational_gas_limit,
            chain_id,
        }
    }
}

pub struct TxSenderInner<G> {
    pub(super) sender_config: TxSenderConfig,
    pub master_connection_pool: Option<ConnectionPool>,
    pub replica_connection_pool: ConnectionPool,
    // Used to keep track of gas prices for the fee ticker.
    pub l1_gas_price_source: Arc<G>,
    pub(super) api_contracts: ApiContracts,
    /// Optional rate limiter that will limit the amount of transactions per second sent from a single entity.
    rate_limiter: Option<TxSenderRateLimiter>,
    /// Optional transaction proxy to be used for transaction submission.
    pub(super) proxy: Option<TxProxy>,
    /// An up-to-date version of the state keeper config.
    /// This field may be omitted on the external node, since the configuration may change unexpectedly.
    /// If this field is set to `None`, `TxSender` will assume that any transaction is executable.
    state_keeper_config: Option<StateKeeperConfig>,
    /// Used to limit the amount of VMs that can be executed simultaneously.
    pub(super) vm_concurrency_limiter: Arc<VmConcurrencyLimiter>,
    // Caches used in VM execution.
    storage_caches: PostgresStorageCaches,
}

pub struct TxSender<G>(pub(super) Arc<TxSenderInner<G>>);

// Custom implementation is required due to generic param:
// Even though it's under `Arc`, compiler doesn't generate the `Clone` implementation unless
// an unnecessary bound is added.
impl<G> Clone for TxSender<G> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<G> std::fmt::Debug for TxSender<G> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxSender").finish()
    }
}

impl<G: L1GasPriceProvider> TxSender<G> {
    pub(crate) fn vm_concurrency_limiter(&self) -> Arc<VmConcurrencyLimiter> {
        Arc::clone(&self.0.vm_concurrency_limiter)
    }

    pub(crate) fn storage_caches(&self) -> PostgresStorageCaches {
        self.0.storage_caches.clone()
    }

    #[tracing::instrument(skip(self, tx))]
    pub async fn submit_tx(&self, tx: L2Tx) -> Result<L2TxSubmissionResult, SubmitTxError> {
        if let Some(rate_limiter) = &self.0.rate_limiter {
            if rate_limiter.check().is_err() {
                return Err(SubmitTxError::RateLimitExceeded);
            }
        }

        let stage_latency = SANDBOX_METRICS.submit_tx[&SubmitTxStage::Validate].start();
        self.validate_tx(&tx).await?;
        stage_latency.observe();

        let stage_latency = SANDBOX_METRICS.submit_tx[&SubmitTxStage::DryRun].start();
        let shared_args = self.shared_args();
        let vm_permit = self.0.vm_concurrency_limiter.acquire().await;
        let vm_permit = vm_permit.ok_or(SubmitTxError::ServerShuttingDown)?;

        let (_, tx_metrics) = execute_tx_with_pending_state(
            vm_permit.clone(),
            shared_args.clone(),
            TxExecutionArgs::for_validation(&tx),
            self.0.replica_connection_pool.clone(),
            tx.clone().into(),
        )
        .await;

        tracing::info!(
            "Submit tx {:?} with execution metrics {:?}",
            tx.hash(),
            tx_metrics
        );
        stage_latency.observe();

        let stage_latency = SANDBOX_METRICS.submit_tx[&SubmitTxStage::VerifyExecute].start();
        let computational_gas_limit = self.0.sender_config.validation_computational_gas_limit;
        let validation_result = shared_args
            .validate_tx_with_pending_state(
                vm_permit,
                self.0.replica_connection_pool.clone(),
                tx.clone(),
                computational_gas_limit,
            )
            .await;
        stage_latency.observe();

        if let Err(err) = validation_result {
            return Err(err.into());
        }

        let stage_started_at = Instant::now();
        self.ensure_tx_executable(tx.clone().into(), &tx_metrics, true)?;

        if let Some(proxy) = &self.0.proxy {
            // We're running an external node: we have to proxy the transaction to the main node.
            // But before we do that, save the tx to cache in case someone will request it
            // Before it reaches the main node.
            proxy.save_tx(tx.hash(), tx.clone()).await;
            proxy.submit_tx(&tx).await?;
            // Now, after we are sure that the tx is on the main node, remove it from cache
            // since we don't want to store txs that might have been replaced or otherwise removed
            // from the mempool.
            proxy.forget_tx(tx.hash()).await;
            SANDBOX_METRICS.submit_tx[&SubmitTxStage::TxProxy].observe(stage_started_at.elapsed());
            APP_METRICS.processed_txs[&TxStage::Proxied].inc();
            return Ok(L2TxSubmissionResult::Proxied);
        } else {
            assert!(
                self.0.master_connection_pool.is_some(),
                "TxSender is instantiated without both master connection pool and tx proxy"
            );
        }

        let nonce = tx.common_data.nonce.0;
        let hash = tx.hash();
        let expected_nonce = self.get_expected_nonce(&tx).await;
        let submission_res_handle = self
            .0
            .master_connection_pool
            .as_ref()
            .unwrap() // Checked above
            .access_storage_tagged("api")
            .await
            .unwrap()
            .transactions_dal()
            .insert_transaction_l2(tx, tx_metrics)
            .await;

        APP_METRICS.processed_txs[&TxStage::Mempool(submission_res_handle)].inc();

        match submission_res_handle {
            L2TxSubmissionResult::AlreadyExecuted => Err(SubmitTxError::NonceIsTooLow(
                expected_nonce.0,
                expected_nonce.0 + self.0.sender_config.max_nonce_ahead,
                nonce,
            )),
            L2TxSubmissionResult::Duplicate => Err(SubmitTxError::IncorrectTx(TxDuplication(hash))),
            _ => {
                SANDBOX_METRICS.submit_tx[&SubmitTxStage::DbInsert]
                    .observe(stage_started_at.elapsed());
                Ok(submission_res_handle)
            }
        }
    }

    fn shared_args(&self) -> TxSharedArgs {
        TxSharedArgs {
            operator_account: AccountTreeId::new(self.0.sender_config.fee_account_addr),
            l1_gas_price: self.0.l1_gas_price_source.estimate_effective_gas_price(),
            fair_l2_gas_price: self.0.sender_config.fair_l2_gas_price,
            base_system_contracts: self.0.api_contracts.eth_call.clone(),
            caches: self.storage_caches(),
            validation_computational_gas_limit: self
                .0
                .sender_config
                .validation_computational_gas_limit,
            chain_id: self.0.sender_config.chain_id,
        }
    }

    async fn validate_tx(&self, tx: &L2Tx) -> Result<(), SubmitTxError> {
        let max_gas = U256::from(u32::MAX);
        if tx.common_data.fee.gas_limit > max_gas
            || tx.common_data.fee.gas_per_pubdata_limit > max_gas
        {
            return Err(SubmitTxError::GasLimitIsTooBig);
        }

        // TODO (SMA-1715): do not subsidize the overhead for the transaction

        if tx.common_data.fee.gas_limit > self.0.sender_config.max_allowed_l2_tx_gas_limit.into() {
            tracing::info!(
                "Submitted Tx is Unexecutable {:?} because of GasLimitIsTooBig {}",
                tx.hash(),
                tx.common_data.fee.gas_limit,
            );
            return Err(SubmitTxError::GasLimitIsTooBig);
        }
        if tx.common_data.fee.max_fee_per_gas < self.0.sender_config.fair_l2_gas_price.into() {
            tracing::info!(
                "Submitted Tx is Unexecutable {:?} because of MaxFeePerGasTooLow {}",
                tx.hash(),
                tx.common_data.fee.max_fee_per_gas
            );
            return Err(SubmitTxError::MaxFeePerGasTooLow);
        }
        if tx.common_data.fee.max_fee_per_gas < tx.common_data.fee.max_priority_fee_per_gas {
            tracing::info!(
                "Submitted Tx is Unexecutable {:?} because of MaxPriorityFeeGreaterThanMaxFee {}",
                tx.hash(),
                tx.common_data.fee.max_fee_per_gas
            );
            return Err(SubmitTxError::MaxPriorityFeeGreaterThanMaxFee);
        }
        if tx.execute.factory_deps_length() > MAX_NEW_FACTORY_DEPS {
            return Err(SubmitTxError::TooManyFactoryDependencies(
                tx.execute.factory_deps_length(),
                MAX_NEW_FACTORY_DEPS,
            ));
        }

        let l1_gas_price = self.0.l1_gas_price_source.estimate_effective_gas_price();
        let (_, gas_per_pubdata_byte) = derive_base_fee_and_gas_per_pubdata(
            l1_gas_price,
            self.0.sender_config.fair_l2_gas_price,
        );
        let effective_gas_per_pubdata = cmp::min(
            tx.common_data.fee.gas_per_pubdata_limit,
            gas_per_pubdata_byte.into(),
        );

        let intrinsic_consts = get_intrinsic_constants();
        let min_gas_limit = U256::from(intrinsic_consts.l2_tx_intrinsic_gas)
            + U256::from(intrinsic_consts.l2_tx_intrinsic_pubdata) * effective_gas_per_pubdata;
        if tx.common_data.fee.gas_limit < min_gas_limit {
            return Err(SubmitTxError::IntrinsicGas);
        }

        // We still double-check the nonce manually
        // to make sure that only the correct nonce is submitted and the transaction's hashes never repeat
        self.validate_account_nonce(tx).await?;
        // Even though without enough balance the tx will not pass anyway
        // we check the user for enough balance explicitly here for better DevEx.
        self.validate_enough_balance(tx).await?;
        Ok(())
    }

    async fn validate_account_nonce(&self, tx: &L2Tx) -> Result<(), SubmitTxError> {
        let expected_nonce = self.get_expected_nonce(tx).await;

        if tx.common_data.nonce.0 < expected_nonce.0 {
            Err(SubmitTxError::NonceIsTooLow(
                expected_nonce.0,
                expected_nonce.0 + self.0.sender_config.max_nonce_ahead,
                tx.nonce().0,
            ))
        } else {
            let max_nonce = expected_nonce.0 + self.0.sender_config.max_nonce_ahead;
            if !(expected_nonce.0..=max_nonce).contains(&tx.common_data.nonce.0) {
                Err(SubmitTxError::NonceIsTooHigh(
                    expected_nonce.0,
                    max_nonce,
                    tx.nonce().0,
                ))
            } else {
                Ok(())
            }
        }
    }

    async fn get_expected_nonce(&self, tx: &L2Tx) -> Nonce {
        let mut connection = self
            .0
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap();

        let latest_block_number = connection
            .blocks_web3_dal()
            .get_sealed_miniblock_number()
            .await
            .unwrap();
        let nonce = connection
            .storage_web3_dal()
            .get_address_historical_nonce(tx.initiator_account(), latest_block_number)
            .await
            .unwrap();
        Nonce(nonce.as_u32())
    }

    async fn validate_enough_balance(&self, tx: &L2Tx) -> Result<(), SubmitTxError> {
        let paymaster = tx.common_data.paymaster_params.paymaster;

        // The paymaster is expected to pay for the tx,
        // whatever balance the user has, we don't care.
        if paymaster != Address::default() {
            return Ok(());
        }

        let balance = self.get_balance(&tx.common_data.initiator_address).await;

        // Estimate the minimum fee price user will agree to.
        let gas_price = cmp::min(
            tx.common_data.fee.max_fee_per_gas,
            U256::from(self.0.sender_config.fair_l2_gas_price)
                + tx.common_data.fee.max_priority_fee_per_gas,
        );
        let max_fee = tx.common_data.fee.gas_limit * gas_price;
        let max_fee_and_value = max_fee + tx.execute.value;

        if balance < max_fee_and_value {
            Err(SubmitTxError::NotEnoughBalanceForFeeValue(
                balance,
                max_fee,
                tx.execute.value,
            ))
        } else {
            Ok(())
        }
    }

    async fn get_balance(&self, initiator_address: &H160) -> U256 {
        let eth_balance_key = storage_key_for_eth_balance(initiator_address);

        let balance = self
            .0
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .storage_dal()
            .get_by_key(&eth_balance_key)
            .await
            .unwrap_or_default();

        h256_to_u256(balance)
    }

    /// Given the gas_limit to be used for the body of the transaction,
    /// returns the result for executing the transaction with such gas_limit
    #[allow(clippy::too_many_arguments)]
    async fn estimate_gas_step(
        &self,
        vm_permit: VmPermit,
        mut tx: Transaction,
        gas_per_pubdata_byte: u64,
        tx_gas_limit: u32,
        l1_gas_price: u64,
        base_fee: u64,
    ) -> (VmExecutionResultAndLogs, TransactionExecutionMetrics) {
        let gas_limit_with_overhead = tx_gas_limit
            + derive_overhead(
                tx_gas_limit,
                gas_per_pubdata_byte as u32,
                tx.encoding_len(),
                OverheadCoeficients::from_tx_type(tx.tx_format() as u8),
            );

        match &mut tx.common_data {
            ExecuteTransactionCommon::L1(l1_common_data) => {
                l1_common_data.gas_limit = gas_limit_with_overhead.into();
                let required_funds =
                    l1_common_data.gas_limit * l1_common_data.max_fee_per_gas + tx.execute.value;
                l1_common_data.to_mint = required_funds;
            }
            ExecuteTransactionCommon::L2(l2_common_data) => {
                l2_common_data.fee.gas_limit = gas_limit_with_overhead.into();
            }
            ExecuteTransactionCommon::ProtocolUpgrade(common_data) => {
                common_data.gas_limit = gas_limit_with_overhead.into();

                let required_funds =
                    common_data.gas_limit * common_data.max_fee_per_gas + tx.execute.value;

                common_data.to_mint = required_funds;
            }
        }

        let shared_args = self.shared_args_for_gas_estimate(l1_gas_price);
        let vm_execution_cache_misses_limit = self.0.sender_config.vm_execution_cache_misses_limit;
        let execution_args =
            TxExecutionArgs::for_gas_estimate(vm_execution_cache_misses_limit, &tx, base_fee);
        let (exec_result, tx_metrics) = execute_tx_with_pending_state(
            vm_permit,
            shared_args,
            execution_args,
            self.0.replica_connection_pool.clone(),
            tx.clone(),
        )
        .await;

        (exec_result, tx_metrics)
    }

    fn shared_args_for_gas_estimate(&self, l1_gas_price: u64) -> TxSharedArgs {
        let config = &self.0.sender_config;
        TxSharedArgs {
            operator_account: AccountTreeId::new(config.fee_account_addr),
            l1_gas_price,
            fair_l2_gas_price: config.fair_l2_gas_price,
            // We want to bypass the computation gas limit check for gas estimation
            validation_computational_gas_limit: BLOCK_GAS_LIMIT,
            base_system_contracts: self.0.api_contracts.estimate_gas.clone(),
            caches: self.storage_caches(),
            chain_id: config.chain_id,
        }
    }

    pub async fn get_txs_fee_in_wei(
        &self,
        mut tx: Transaction,
        estimated_fee_scale_factor: f64,
        acceptable_overestimation: u32,
    ) -> Result<Fee, SubmitTxError> {
        let estimation_started_at = Instant::now();
        let l1_gas_price = {
            let effective_gas_price = self.0.l1_gas_price_source.estimate_effective_gas_price();
            let current_l1_gas_price =
                ((effective_gas_price as f64) * self.0.sender_config.gas_price_scale_factor) as u64;

            // In order for execution to pass smoothly, we need to ensure that block's required gasPerPubdata will be
            // <= to the one in the transaction itself.
            adjust_l1_gas_price_for_tx(
                current_l1_gas_price,
                self.0.sender_config.fair_l2_gas_price,
                tx.gas_per_pubdata_byte_limit(),
            )
        };

        let (base_fee, gas_per_pubdata_byte) = derive_base_fee_and_gas_per_pubdata(
            l1_gas_price,
            self.0.sender_config.fair_l2_gas_price,
        );
        match &mut tx.common_data {
            ExecuteTransactionCommon::L2(common_data) => {
                common_data.fee.max_fee_per_gas = base_fee.into();
                common_data.fee.max_priority_fee_per_gas = base_fee.into();
            }
            ExecuteTransactionCommon::L1(common_data) => {
                common_data.max_fee_per_gas = base_fee.into();
            }
            ExecuteTransactionCommon::ProtocolUpgrade(common_data) => {
                common_data.max_fee_per_gas = base_fee.into();
            }
        }

        let hashed_key = get_code_key(&tx.initiator_account());
        // if the default account does not have enough funds
        // for transferring tx.value, without taking into account the fee,
        // there is no sense to estimate the fee
        let account_code_hash = self
            .0
            .replica_connection_pool
            .access_storage_tagged("api")
            .await
            .unwrap()
            .storage_dal()
            .get_by_key(&hashed_key)
            .await
            .unwrap_or_default();

        if !tx.is_l1()
            && account_code_hash == H256::zero()
            && tx.execute.value > self.get_balance(&tx.initiator_account()).await
        {
            tracing::info!(
                "fee estimation failed on validation step.
                account: {} does not have enough funds for for transferring tx.value: {}.",
                &tx.initiator_account(),
                tx.execute.value
            );
            return Err(SubmitTxError::InsufficientFundsForTransfer);
        }

        // For L2 transactions we need a properly formatted signature
        if let ExecuteTransactionCommon::L2(l2_common_data) = &mut tx.common_data {
            if l2_common_data.signature.is_empty() {
                l2_common_data.signature = PackedEthSignature::default().serialize_packed().into();
            }

            l2_common_data.fee.gas_per_pubdata_limit = MAX_GAS_PER_PUBDATA_BYTE.into();
        }

        // Acquire the vm token for the whole duration of the binary search.
        let vm_permit = self.0.vm_concurrency_limiter.acquire().await;
        let vm_permit = vm_permit.ok_or(SubmitTxError::ServerShuttingDown)?;

        // We already know how many gas is needed to cover for the publishing of the bytecodes.
        // For L1->L2 transactions all the bytecodes have been made available on L1, so no funds need to be
        // spent on re-publishing those.
        let gas_for_bytecodes_pubdata = if tx.is_l1() {
            0
        } else {
            let pubdata_for_factory_deps = get_pubdata_for_factory_deps(
                &vm_permit,
                &self.0.replica_connection_pool,
                tx.execute.factory_deps.as_deref().unwrap_or_default(),
                self.storage_caches(),
            )
            .await;

            if pubdata_for_factory_deps > MAX_PUBDATA_PER_BLOCK {
                return Err(SubmitTxError::Unexecutable(
                    "exceeds limit for published pubdata".to_string(),
                ));
            }
            pubdata_for_factory_deps * (gas_per_pubdata_byte as u32)
        };

        // We are using binary search to find the minimal values of gas_limit under which
        // the transaction succeedes
        let mut lower_bound = 0;
        let mut upper_bound = MAX_L2_TX_GAS_LIMIT as u32;
        let tx_id = format!(
            "{:?}-{}",
            tx.initiator_account(),
            tx.nonce().unwrap_or(Nonce(0))
        );
        tracing::trace!(
            "fee estimation tx {:?}: preparation took {:?}, starting binary search",
            tx_id,
            estimation_started_at.elapsed(),
        );

        let mut number_of_iterations = 0usize;
        while lower_bound + acceptable_overestimation < upper_bound {
            let mid = (lower_bound + upper_bound) / 2;
            // There is no way to distinct between errors due to out of gas
            // or normal exeuction errors, so we just hope that increasing the
            // gas limit will make the transaction successful
            let iteration_started_at = Instant::now();
            let try_gas_limit = gas_for_bytecodes_pubdata + mid;
            let (result, _execution_metrics) = self
                .estimate_gas_step(
                    vm_permit.clone(),
                    tx.clone(),
                    gas_per_pubdata_byte,
                    try_gas_limit,
                    l1_gas_price,
                    base_fee,
                )
                .await;

            if result.result.is_failed() {
                lower_bound = mid + 1;
            } else {
                upper_bound = mid;
            }

            tracing::trace!(
                "fee estimation tx {:?}: iteration {} took {:?}. lower_bound: {}, upper_bound: {}",
                tx_id,
                number_of_iterations,
                iteration_started_at.elapsed(),
                lower_bound,
                upper_bound,
            );
            number_of_iterations += 1;
        }
        SANDBOX_METRICS
            .estimate_gas_binary_search_iterations
            .observe(number_of_iterations);

        let tx_body_gas_limit = cmp::min(
            MAX_L2_TX_GAS_LIMIT as u32,
            ((upper_bound as f64) * estimated_fee_scale_factor) as u32,
        );

        let suggested_gas_limit = tx_body_gas_limit + gas_for_bytecodes_pubdata;
        let (result, tx_metrics) = self
            .estimate_gas_step(
                vm_permit,
                tx.clone(),
                gas_per_pubdata_byte,
                suggested_gas_limit,
                l1_gas_price,
                base_fee,
            )
            .await;

        result.into_api_call_result()?;
        self.ensure_tx_executable(tx.clone(), &tx_metrics, false)?;

        let overhead = derive_overhead(
            suggested_gas_limit,
            gas_per_pubdata_byte as u32,
            tx.encoding_len(),
            OverheadCoeficients::from_tx_type(tx.tx_format() as u8),
        );

        let full_gas_limit =
            match tx_body_gas_limit.overflowing_add(gas_for_bytecodes_pubdata + overhead) {
                (value, false) => value,
                (_, true) => {
                    return Err(SubmitTxError::ExecutionReverted(
                        "exceeds block gas limit".to_string(),
                        vec![],
                    ));
                }
            };

        Ok(Fee {
            max_fee_per_gas: base_fee.into(),
            max_priority_fee_per_gas: 0u32.into(),
            gas_limit: full_gas_limit.into(),
            gas_per_pubdata_limit: gas_per_pubdata_byte.into(),
        })
    }

    pub(super) async fn eth_call(
        &self,
        block_args: BlockArgs,
        tx: L2Tx,
    ) -> Result<Vec<u8>, SubmitTxError> {
        let vm_permit = self.0.vm_concurrency_limiter.acquire().await;
        let vm_permit = vm_permit.ok_or(SubmitTxError::ServerShuttingDown)?;

        let vm_execution_cache_misses_limit = self.0.sender_config.vm_execution_cache_misses_limit;
        execute_tx_eth_call(
            vm_permit,
            self.shared_args(),
            self.0.replica_connection_pool.clone(),
            tx,
            block_args,
            vm_execution_cache_misses_limit,
            vec![],
        )
        .await
        .into_api_call_result()
    }

    pub fn gas_price(&self) -> u64 {
        let gas_price = self.0.l1_gas_price_source.estimate_effective_gas_price();
        let l1_gas_price = (gas_price as f64 * self.0.sender_config.gas_price_scale_factor).round();
        let (base_fee, _) = derive_base_fee_and_gas_per_pubdata(
            l1_gas_price as u64,
            self.0.sender_config.fair_l2_gas_price,
        );
        base_fee
    }

    fn ensure_tx_executable(
        &self,
        transaction: Transaction,
        tx_metrics: &TransactionExecutionMetrics,
        log_message: bool,
    ) -> Result<(), SubmitTxError> {
        let Some(sk_config) = &self.0.state_keeper_config else {
            // No config provided, so we can't check if transaction satisfies the seal criteria.
            // We assume that it's executable, and if it's not, it will be caught by the main server
            // (where this check is always performed).
            return Ok(());
        };

        // Hash is not computable for the provided `transaction` during gas estimation (it doesn't have
        // its input data set). Since we don't log a hash in this case anyway, we just use a dummy value.
        let tx_hash = if log_message {
            transaction.hash()
        } else {
            H256::zero()
        };

        // Using `ProtocolVersionId::latest()` for a short period we might end up in a scenario where the StateKeeper is still pre-boojum
        // but the API assumes we are post boojum. In this situation we will determine a tx as being executable but the StateKeeper will
        // still reject them as it's not.
        let protocol_version = ProtocolVersionId::latest();
        let seal_data = SealData::for_transaction(transaction, tx_metrics, protocol_version);
        if let Some(reason) =
            ConditionalSealer::find_unexecutable_reason(sk_config, &seal_data, protocol_version)
        {
            let message = format!(
                "Tx is Unexecutable because of {reason}; inputs for decision: {seal_data:?}"
            );
            if log_message {
                tracing::info!("{tx_hash:#?} {message}");
            }
            return Err(SubmitTxError::Unexecutable(message));
        }
        Ok(())
    }
}
