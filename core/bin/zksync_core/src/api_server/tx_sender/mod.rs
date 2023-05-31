//! Helper module to submit transactions into the zkSync Network.
// Built-in uses
use std::{cmp::min, num::NonZeroU32, sync::Arc, time::Instant};

// External uses
use governor::clock::MonotonicClock;
use governor::middleware::NoOpMiddleware;
use governor::state::{InMemoryState, NotKeyed};
use governor::{Quota, RateLimiter};

use vm::vm_with_bootloader::{derive_base_fee_and_gas_per_pubdata, TxExecutionMode};
use vm::zk_evm::zkevm_opcode_defs::system_params::MAX_PUBDATA_PER_BLOCK;
use zksync_config::configs::chain::StateKeeperConfig;
use zksync_contracts::{
    BaseSystemContracts, SystemContractCode, ESTIMATE_FEE_BLOCK_CODE,
    PLAYGROUND_BLOCK_BOOTLOADER_CODE,
};
use zksync_dal::transactions_dal::L2TxSubmissionResult;

use vm::transaction_data::TransactionData;
use zksync_config::ZkSyncConfig;
use zksync_types::fee::TransactionExecutionMetrics;

use zksync_types::{
    ExecuteTransactionCommon, Transaction, MAX_GAS_PER_PUBDATA_BYTE, MAX_L2_TX_GAS_LIMIT,
    MAX_NEW_FACTORY_DEPS,
};

use zksync_dal::ConnectionPool;

use zksync_types::{
    api,
    fee::Fee,
    get_code_key, get_intrinsic_constants,
    l2::error::TxCheckError::TxDuplication,
    l2::L2Tx,
    tx::tx_execution_info::{DeduplicatedWritesMetrics, ExecutionMetrics},
    utils::storage_key_for_eth_balance,
    AccountTreeId, Address, Nonce, H160, H256, U256,
};

use zksync_utils::{bytes_to_be_words, h256_to_u256};

// Local uses
use crate::api_server::execution_sandbox::{
    adjust_l1_gas_price_for_tx, execute_tx_with_pending_state, get_pubdata_for_factory_deps,
    validate_tx_with_pending_state, SandboxExecutionError,
};

use crate::gas_tracker::{gas_count_from_tx_and_metrics, gas_count_from_writes};
use crate::l1_gas_price::L1GasPriceProvider;
use crate::state_keeper::seal_criteria::conditional_sealer::ConditionalSealer;
use crate::state_keeper::seal_criteria::SealResolution;

pub mod error;
pub use error::SubmitTxError;
use vm::transaction_data::{derive_overhead, OverheadCoeficients};

pub mod proxy;
pub use proxy::TxProxy;

/// Type alias for the rate limiter implementation.
type TxSenderRateLimiter =
    RateLimiter<NotKeyed, InMemoryState, MonotonicClock, NoOpMiddleware<Instant>>;

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
            &MonotonicClock::default(),
        );
        Self {
            rate_limiter: Some(rate_limiter),
            ..self
        }
    }

    pub fn with_tx_proxy(mut self, main_node_url: String) -> Self {
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

    pub fn build<G: L1GasPriceProvider>(
        self,
        l1_gas_price_source: Arc<G>,
        default_aa_hash: H256,
    ) -> TxSender<G> {
        assert!(
            self.master_connection_pool.is_some() || self.proxy.is_some(),
            "Either master connection pool or proxy must be set"
        );

        let mut storage = self.replica_connection_pool.access_storage_blocking();
        let default_aa_bytecode = storage
            .storage_dal()
            .get_factory_dep(default_aa_hash)
            .expect("Default AA hash must be present in the database");
        drop(storage);

        let default_aa_contract = SystemContractCode {
            code: bytes_to_be_words(default_aa_bytecode),
            hash: default_aa_hash,
        };

        let playground_base_system_contracts = BaseSystemContracts {
            default_aa: default_aa_contract.clone(),
            bootloader: PLAYGROUND_BLOCK_BOOTLOADER_CODE.clone(),
        };

        let estimate_fee_base_system_contracts = BaseSystemContracts {
            default_aa: default_aa_contract,
            bootloader: ESTIMATE_FEE_BLOCK_CODE.clone(),
        };

        TxSender(Arc::new(TxSenderInner {
            sender_config: self.config,
            master_connection_pool: self.master_connection_pool,
            replica_connection_pool: self.replica_connection_pool,
            l1_gas_price_source,
            playground_base_system_contracts,
            estimate_fee_base_system_contracts,
            rate_limiter: self.rate_limiter,
            proxy: self.proxy,
            state_keeper_config: self.state_keeper_config,
        }))
    }
}

/// Internal static `TxSender` configuration.
/// This structure is detached from `ZkSyncConfig`, since different node types (main, external, etc)
/// may require different configuration layouts.
/// The intention is to only keep the actually used information here.
#[derive(Debug)]
pub struct TxSenderConfig {
    pub fee_account_addr: Address,
    pub gas_price_scale_factor: f64,
    pub max_nonce_ahead: u32,
    pub max_allowed_l2_tx_gas_limit: u32,
    pub fair_l2_gas_price: u64,
    pub vm_execution_cache_misses_limit: Option<usize>,
    pub validation_computational_gas_limit: u32,
}

impl From<ZkSyncConfig> for TxSenderConfig {
    fn from(config: ZkSyncConfig) -> Self {
        Self {
            fee_account_addr: config.chain.state_keeper.fee_account_addr,
            gas_price_scale_factor: config.api.web3_json_rpc.gas_price_scale_factor,
            max_nonce_ahead: config.api.web3_json_rpc.max_nonce_ahead,
            max_allowed_l2_tx_gas_limit: config.chain.state_keeper.max_allowed_l2_tx_gas_limit,
            fair_l2_gas_price: config.chain.state_keeper.fair_l2_gas_price,
            vm_execution_cache_misses_limit: config
                .api
                .web3_json_rpc
                .vm_execution_cache_misses_limit,
            validation_computational_gas_limit: config
                .chain
                .state_keeper
                .validation_computational_gas_limit,
        }
    }
}

pub struct TxSenderInner<G> {
    pub(super) sender_config: TxSenderConfig,
    pub master_connection_pool: Option<ConnectionPool>,
    pub replica_connection_pool: ConnectionPool,
    // Used to keep track of gas prices for the fee ticker.
    pub l1_gas_price_source: Arc<G>,
    pub(super) playground_base_system_contracts: BaseSystemContracts,
    estimate_fee_base_system_contracts: BaseSystemContracts,
    /// Optional rate limiter that will limit the amount of transactions per second sent from a single entity.
    rate_limiter: Option<TxSenderRateLimiter>,
    /// Optional transaction proxy to be used for transaction submission.
    pub(super) proxy: Option<TxProxy>,
    /// An up-to-date version of the state keeper config.
    /// This field may be omitted on the external node, since the configuration may change unexpectedly.
    /// If this field is set to `None`, `TxSender` will assume that any transaction is executable.
    state_keeper_config: Option<StateKeeperConfig>,
}

pub struct TxSender<G>(pub Arc<TxSenderInner<G>>);

// Custom implementation is required due to generic param:
// Even though it's under `Arc`, compiler doesn't generate the `Clone` implementation unless
// an unnecessary bound is added.
impl<E> Clone for TxSender<E> {
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
    #[tracing::instrument(skip(self, tx))]
    pub fn submit_tx(&self, tx: L2Tx) -> Result<L2TxSubmissionResult, SubmitTxError> {
        if let Some(rate_limiter) = &self.0.rate_limiter {
            if rate_limiter.check().is_err() {
                return Err(SubmitTxError::RateLimitExceeded);
            }
        }
        let mut stage_started_at = Instant::now();

        if tx.common_data.fee.gas_limit > U256::from(u32::MAX)
            || tx.common_data.fee.gas_per_pubdata_limit > U256::from(u32::MAX)
        {
            return Err(SubmitTxError::GasLimitIsTooBig);
        }

        let _maximal_allowed_overhead = 0;

        if tx.common_data.fee.gas_limit
            > U256::from(self.0.sender_config.max_allowed_l2_tx_gas_limit)
        {
            vlog::info!(
                "Submitted Tx is Unexecutable {:?} because of GasLimitIsTooBig {}",
                tx.hash(),
                tx.common_data.fee.gas_limit,
            );
            return Err(SubmitTxError::GasLimitIsTooBig);
        }
        if tx.common_data.fee.max_fee_per_gas < self.0.sender_config.fair_l2_gas_price.into() {
            vlog::info!(
                "Submitted Tx is Unexecutable {:?} because of MaxFeePerGasTooLow {}",
                tx.hash(),
                tx.common_data.fee.max_fee_per_gas
            );
            return Err(SubmitTxError::MaxFeePerGasTooLow);
        }
        if tx.common_data.fee.max_fee_per_gas < tx.common_data.fee.max_priority_fee_per_gas {
            vlog::info!(
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

        let intrinsic_constants = get_intrinsic_constants();
        if tx.common_data.fee.gas_limit
            < U256::from(intrinsic_constants.l2_tx_intrinsic_gas)
                + U256::from(intrinsic_constants.l2_tx_intrinsic_pubdata)
                    * min(
                        U256::from(gas_per_pubdata_byte),
                        tx.common_data.fee.gas_per_pubdata_limit,
                    )
        {
            return Err(SubmitTxError::IntrinsicGas);
        }

        // We still double-check the nonce manually
        // to make sure that only the correct nonce is submitted and the transaction's hashes never repeat
        self.validate_account_nonce(&tx)?;

        // Even though without enough balance the tx will not pass anyway
        // we check the user for enough balance explicitly here for better DevEx.
        self.validate_enough_balance(&tx)?;

        metrics::histogram!("api.web3.submit_tx", stage_started_at.elapsed(), "stage" => "1_validate");
        stage_started_at = Instant::now();

        let l1_gas_price = self.0.l1_gas_price_source.estimate_effective_gas_price();
        let fair_l2_gas_price = self.0.sender_config.fair_l2_gas_price;

        let (tx_metrics, _) = execute_tx_with_pending_state(
            &self.0.replica_connection_pool,
            tx.clone().into(),
            AccountTreeId::new(self.0.sender_config.fee_account_addr),
            TxExecutionMode::VerifyExecute,
            Some(tx.nonce()),
            U256::zero(),
            l1_gas_price,
            fair_l2_gas_price,
            Some(tx.common_data.fee.max_fee_per_gas.as_u64()),
            &self.0.playground_base_system_contracts,
            &mut Default::default(),
        );

        vlog::info!(
            "Submit tx {:?} with execution metrics {:?}",
            tx.hash(),
            tx_metrics
        );
        metrics::histogram!("api.web3.submit_tx", stage_started_at.elapsed(), "stage" => "2_dry_run");
        stage_started_at = Instant::now();

        let validation_result = validate_tx_with_pending_state(
            &self.0.replica_connection_pool,
            tx.clone(),
            AccountTreeId::new(self.0.sender_config.fee_account_addr),
            TxExecutionMode::VerifyExecute,
            Some(tx.nonce()),
            U256::zero(),
            l1_gas_price,
            fair_l2_gas_price,
            Some(tx.common_data.fee.max_fee_per_gas.as_u64()),
            &self.0.playground_base_system_contracts,
            self.0.sender_config.validation_computational_gas_limit,
        );

        metrics::histogram!("api.web3.submit_tx", stage_started_at.elapsed(), "stage" => "3_verify_execute");
        stage_started_at = Instant::now();

        if let Err(err) = validation_result {
            return Err(err.into());
        }

        self.ensure_tx_executable(&tx.clone().into(), &tx_metrics, true)?;

        if let Some(proxy) = &self.0.proxy {
            // We're running an external node: we have to proxy the transaction to the main node.
            // But before we do that, save the tx to cache in case someone will request it
            // Before it reaches the main node.
            proxy.save_tx(tx.hash(), tx.clone());
            proxy.submit_tx(&tx)?;
            // Now, after we are sure that the tx is on the main node, remove it from cache
            // since we don't want to store txs that might have been replaced or otherwise removed
            // from the mempool.
            proxy.forget_tx(tx.hash());
            metrics::histogram!("api.web3.submit_tx", stage_started_at.elapsed(), "stage" => "4_tx_proxy");
            metrics::counter!("server.processed_txs", 1, "stage" => "proxied");
            return Ok(L2TxSubmissionResult::Proxied);
        } else {
            assert!(
                self.0.master_connection_pool.is_some(),
                "TxSender is instantiated without both master connection pool and tx proxy"
            );
        }

        let nonce = tx.common_data.nonce.0;
        let hash = tx.hash();
        let expected_nonce = self.get_expected_nonce(&tx);
        let submission_res_handle = self
            .0
            .master_connection_pool
            .as_ref()
            .unwrap() // Checked above
            .access_storage_blocking()
            .transactions_dal()
            .insert_transaction_l2(tx, tx_metrics);

        let status: String;
        let submission_result = match submission_res_handle {
            L2TxSubmissionResult::AlreadyExecuted => {
                status = "already_executed".to_string();
                Err(SubmitTxError::NonceIsTooLow(
                    expected_nonce.0,
                    expected_nonce.0 + self.0.sender_config.max_nonce_ahead,
                    nonce,
                ))
            }
            L2TxSubmissionResult::Duplicate => {
                status = "duplicated".to_string();
                Err(SubmitTxError::IncorrectTx(TxDuplication(hash)))
            }
            _ => {
                metrics::histogram!("api.web3.submit_tx", stage_started_at.elapsed(), "stage" => "4_db_insert");
                status = format!(
                    "mempool_{}",
                    submission_res_handle.to_string().to_lowercase()
                );
                Ok(submission_res_handle)
            }
        };

        metrics::counter!(
            "server.processed_txs",
            1,
            "stage" => status
        );

        submission_result
    }

    fn validate_account_nonce(&self, tx: &L2Tx) -> Result<(), SubmitTxError> {
        let expected_nonce = self.get_expected_nonce(tx);

        if tx.common_data.nonce.0 < expected_nonce.0 {
            Err(SubmitTxError::NonceIsTooLow(
                expected_nonce.0,
                expected_nonce.0 + self.0.sender_config.max_nonce_ahead,
                tx.nonce().0,
            ))
        } else if !(expected_nonce.0..=(expected_nonce.0 + self.0.sender_config.max_nonce_ahead))
            .contains(&tx.common_data.nonce.0)
        {
            Err(SubmitTxError::NonceIsTooHigh(
                expected_nonce.0,
                expected_nonce.0 + self.0.sender_config.max_nonce_ahead,
                tx.nonce().0,
            ))
        } else {
            Ok(())
        }
    }

    fn get_expected_nonce(&self, tx: &L2Tx) -> Nonce {
        self.0
            .replica_connection_pool
            .access_storage_blocking()
            .storage_web3_dal()
            .get_address_historical_nonce(
                tx.initiator_account(),
                api::BlockId::Number(api::BlockNumber::Latest),
            )
            .unwrap()
            .map(|n| Nonce(n.as_u32()))
            .unwrap()
    }

    fn validate_enough_balance(&self, tx: &L2Tx) -> Result<(), SubmitTxError> {
        let paymaster = tx.common_data.paymaster_params.paymaster;

        // The paymaster is expected to pay for the tx,
        // whatever balance the user has, we don't care.
        if paymaster != Address::default() {
            return Ok(());
        }

        let balance = self.get_balance(&tx.common_data.initiator_address);

        // Estimate the minimum fee price user will agree to.
        let gas_price = std::cmp::min(
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

    fn get_balance(&self, initiator_address: &H160) -> U256 {
        let eth_balance_key = storage_key_for_eth_balance(initiator_address);

        let balance = self
            .0
            .replica_connection_pool
            .access_storage_blocking()
            .storage_dal()
            .get_by_key(&eth_balance_key)
            .unwrap_or_default();

        h256_to_u256(balance)
    }

    pub fn get_txs_fee_in_wei(
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
        }

        let hashed_key = get_code_key(&tx.initiator_account());
        // if the default account does not have enough funds
        // for transferring tx.value, without taking into account the fee,
        // there is no sense to estimate the fee
        let account_code_hash = self
            .0
            .replica_connection_pool
            .access_storage_blocking()
            .storage_dal()
            .get_by_key(&hashed_key)
            .unwrap_or_default();

        if !tx.is_l1()
            && account_code_hash == H256::zero()
            && tx.execute.value > self.get_balance(&tx.initiator_account())
        {
            vlog::info!(
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
                l2_common_data.signature = vec![0u8; 65];
                l2_common_data.signature[64] = 27;
            }

            l2_common_data.fee.gas_per_pubdata_limit = MAX_GAS_PER_PUBDATA_BYTE.into();
        }

        // We already know how many gas is needed to cover for the publishing of the bytecodes.
        // For L1->L2 transactions all the bytecodes have been made available on L1, so no funds need to be
        // spent on re-publishing those.
        let gas_for_bytecodes_pubdata = if tx.is_l1() {
            0
        } else {
            let pubdata_for_factory_deps = get_pubdata_for_factory_deps(
                &self.0.replica_connection_pool,
                &tx.execute.factory_deps,
            );
            if pubdata_for_factory_deps > MAX_PUBDATA_PER_BLOCK {
                return Err(SubmitTxError::Unexecutable(
                    "exceeds limit for published pubdata".to_string(),
                ));
            }
            pubdata_for_factory_deps * (gas_per_pubdata_byte as u32)
        };

        // Rolling cache with storage values that were read from the DB.
        let mut storage_read_cache = Default::default();

        // We are using binary search to find the minimal values of gas_limit under which
        // the transaction succeedes
        let mut lower_bound = 0;
        let mut upper_bound = MAX_L2_TX_GAS_LIMIT as u32;
        let tx_id = format!(
            "{:?}-{}",
            tx.initiator_account(),
            tx.nonce().unwrap_or(Nonce(0))
        );
        vlog::trace!(
            "fee estimation tx {:?}: preparation took {:?}, starting binary search",
            tx_id,
            estimation_started_at.elapsed(),
        );
        // Given the gas_limit to be used for the body of the transaction,
        // returns the result for executing the transaction with such gas_limit
        let mut execute = |tx_gas_limit: u32| {
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

                    let required_funds = l1_common_data.gas_limit * l1_common_data.max_fee_per_gas
                        + tx.execute.value;

                    l1_common_data.to_mint = required_funds;
                }
                ExecuteTransactionCommon::L2(l2_common_data) => {
                    l2_common_data.fee.gas_limit = gas_limit_with_overhead.into();
                }
            }

            let enforced_nonce = match &tx.common_data {
                ExecuteTransactionCommon::L2(data) => Some(data.nonce),
                _ => None,
            };

            // For L2 transactions we need to explicitly put enough balance into the account of the users
            // while for L1->L2 transactions the `to_mint` field plays this role
            let added_balance = match &tx.common_data {
                ExecuteTransactionCommon::L2(data) => data.fee.gas_limit * data.fee.max_fee_per_gas,
                _ => U256::zero(),
            };

            let (tx_metrics, exec_result) = execute_tx_with_pending_state(
                &self.0.replica_connection_pool,
                tx.clone(),
                AccountTreeId::new(self.0.sender_config.fee_account_addr),
                TxExecutionMode::EstimateFee {
                    missed_storage_invocation_limit: self
                        .0
                        .sender_config
                        .vm_execution_cache_misses_limit
                        .unwrap_or(usize::MAX),
                },
                enforced_nonce,
                added_balance,
                l1_gas_price,
                self.0.sender_config.fair_l2_gas_price,
                Some(base_fee),
                &self.0.estimate_fee_base_system_contracts,
                &mut storage_read_cache,
            );

            self.ensure_tx_executable(&tx, &tx_metrics, false)
                .map_err(|err| {
                    let err_message = match err {
                        SubmitTxError::Unexecutable(err_message) => err_message,
                        _ => unreachable!(),
                    };

                    SandboxExecutionError::Unexecutable(err_message)
                })?;

            exec_result
        };
        let mut number_of_iterations = 0usize;
        while lower_bound + acceptable_overestimation < upper_bound {
            let mid = (lower_bound + upper_bound) / 2;
            // There is no way to distinct between errors due to out of gas
            // or normal exeuction errors, so we just hope that increasing the
            // gas limit will make the transaction successful
            let iteration_started_at = Instant::now();
            if execute(gas_for_bytecodes_pubdata + mid).is_err() {
                lower_bound = mid + 1;
            } else {
                upper_bound = mid;
            }

            vlog::trace!(
                "fee estimation tx {:?}: iteration {} took {:?}. lower_bound: {}, upper_bound: {}",
                tx_id,
                number_of_iterations,
                iteration_started_at.elapsed(),
                lower_bound,
                upper_bound,
            );
            number_of_iterations += 1;
        }
        metrics::histogram!(
            "api.web3.estimate_gas_binary_search_iterations",
            number_of_iterations as f64
        );

        let tx_body_gas_limit = std::cmp::min(
            MAX_L2_TX_GAS_LIMIT as u32,
            ((upper_bound as f64) * estimated_fee_scale_factor) as u32,
        );

        match execute(tx_body_gas_limit + gas_for_bytecodes_pubdata) {
            Err(err) => Err(err.into()),
            Ok(_) => {
                let overhead = derive_overhead(
                    tx_body_gas_limit + gas_for_bytecodes_pubdata,
                    gas_per_pubdata_byte as u32,
                    tx.encoding_len(),
                    OverheadCoeficients::from_tx_type(tx.tx_format() as u8),
                );

                let full_gas_limit =
                    match tx_body_gas_limit.overflowing_add(gas_for_bytecodes_pubdata + overhead) {
                        (_, true) => {
                            return Err(SubmitTxError::ExecutionReverted(
                                "exceeds block gas limit".to_string(),
                                vec![],
                            ))
                        }
                        (x, _) => x,
                    };

                Ok(Fee {
                    max_fee_per_gas: base_fee.into(),
                    max_priority_fee_per_gas: 0u32.into(),
                    gas_limit: full_gas_limit.into(),
                    gas_per_pubdata_limit: gas_per_pubdata_byte.into(),
                })
            }
        }
    }

    pub fn gas_price(&self) -> u64 {
        let gas_price = self.0.l1_gas_price_source.estimate_effective_gas_price();

        derive_base_fee_and_gas_per_pubdata(
            (gas_price as f64 * self.0.sender_config.gas_price_scale_factor).round() as u64,
            self.0.sender_config.fair_l2_gas_price,
        )
        .0
    }

    fn ensure_tx_executable(
        &self,
        transaction: &Transaction,
        tx_metrics: &TransactionExecutionMetrics,
        log_message: bool,
    ) -> Result<(), SubmitTxError> {
        let Some(sk_config) = &self.0.state_keeper_config else {
            // No config provided, so we can't check if transaction satisfies the seal criteria.
            // We assume that it's executable, and if it's not, it will be caught by the main server
            // (where this check is always performed).
            return Ok(());
        };

        let execution_metrics = ExecutionMetrics {
            published_bytecode_bytes: tx_metrics.published_bytecode_bytes,
            l2_l1_long_messages: tx_metrics.l2_l1_long_messages,
            l2_l1_logs: tx_metrics.l2_l1_logs,
            contracts_deployed: tx_metrics.contracts_deployed,
            contracts_used: tx_metrics.contracts_used,
            gas_used: tx_metrics.gas_used,
            storage_logs: tx_metrics.storage_logs,
            vm_events: tx_metrics.vm_events,
            total_log_queries: tx_metrics.total_log_queries,
            cycles_used: tx_metrics.cycles_used,
            computational_gas_used: tx_metrics.computational_gas_used,
        };
        let writes_metrics = DeduplicatedWritesMetrics {
            initial_storage_writes: tx_metrics.initial_storage_writes,
            repeated_storage_writes: tx_metrics.repeated_storage_writes,
        };

        // In api server it's ok to expect that all writes are initial it's safer
        let tx_gas_count = gas_count_from_tx_and_metrics(&transaction.clone(), &execution_metrics)
            + gas_count_from_writes(&writes_metrics);
        let tx_data: TransactionData = transaction.clone().into();
        let tx_encoding_size = tx_data.into_tokens().len();

        for sealer in &ConditionalSealer::get_default_sealers() {
            let seal_resolution = sealer.should_seal(
                sk_config,
                0u128,
                1,
                execution_metrics,
                execution_metrics,
                tx_gas_count,
                tx_gas_count,
                tx_encoding_size,
                tx_encoding_size,
                writes_metrics,
                writes_metrics,
            );
            if matches!(seal_resolution, SealResolution::Unexecutable(_)) {
                let message = format!(
                    "Tx is Unexecutable because of {} with execution values {:?} and gas {:?}",
                    sealer.prom_criterion_name(),
                    execution_metrics,
                    tx_gas_count
                );

                if log_message {
                    vlog::info!("{:#?} {}", transaction.hash(), message);
                }

                return Err(SubmitTxError::Unexecutable(message));
            }
        }
        Ok(())
    }
}
