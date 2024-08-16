//! Helper module to submit transactions into the ZKsync Network.

use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use tokio::sync::RwLock;
use zksync_config::configs::{api::Web3JsonRpcConfig, chain::StateKeeperConfig};
use zksync_contracts::BaseSystemContracts;
use zksync_dal::{
    transactions_dal::L2TxSubmissionResult, Connection, ConnectionPool, Core, CoreDal,
};
use zksync_multivm::{
    interface::{TransactionExecutionMetrics, TxExecutionMode, VmExecutionResultAndLogs},
    utils::{
        adjust_pubdata_price_for_tx, derive_base_fee_and_gas_per_pubdata, derive_overhead,
        get_eth_call_gas_limit, get_max_batch_gas_limit,
    },
    vm_latest::constants::BATCH_COMPUTATIONAL_GAS_LIMIT,
};
use zksync_node_fee_model::{ApiFeeInputProvider, BatchFeeModelInputProvider};
use zksync_state::PostgresStorageCaches;
use zksync_state_keeper::{
    seal_criteria::{ConditionalSealer, NoopSealer, SealData},
    SequencerSealer,
};
use zksync_types::{
    api::state_override::StateOverride,
    fee::Fee,
    fee_model::BatchFeeInput,
    get_code_key, get_intrinsic_constants,
    l2::{error::TxCheckError::TxDuplication, L2Tx},
    transaction_request::CallOverrides,
    utils::storage_key_for_eth_balance,
    vm::VmVersion,
    AccountTreeId, Address, ExecuteTransactionCommon, L2ChainId, Nonce, PackedEthSignature,
    ProtocolVersionId, Transaction, H160, H256, MAX_L2_TX_GAS_LIMIT, MAX_NEW_FACTORY_DEPS, U256,
};
use zksync_utils::h256_to_u256;

pub(super) use self::result::SubmitTxError;
use self::{master_pool_sink::MasterPoolSink, tx_sink::TxSink};
use crate::{
    execution_sandbox::{
        BlockArgs, SubmitTxStage, TransactionExecutor, TxExecutionArgs, TxSetupArgs,
        VmConcurrencyBarrier, VmConcurrencyLimiter, VmPermit, SANDBOX_METRICS,
    },
    tx_sender::result::ApiCallResult,
};

pub mod master_pool_sink;
pub mod proxy;
mod result;
#[cfg(test)]
pub(crate) mod tests;
pub mod tx_sink;

pub async fn build_tx_sender(
    tx_sender_config: &TxSenderConfig,
    web3_json_config: &Web3JsonRpcConfig,
    state_keeper_config: &StateKeeperConfig,
    replica_pool: ConnectionPool<Core>,
    master_pool: ConnectionPool<Core>,
    batch_fee_model_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    storage_caches: PostgresStorageCaches,
) -> anyhow::Result<(TxSender, VmConcurrencyBarrier)> {
    let sequencer_sealer = SequencerSealer::new(state_keeper_config.clone());
    let master_pool_sink = MasterPoolSink::new(master_pool);
    let tx_sender_builder = TxSenderBuilder::new(
        tx_sender_config.clone(),
        replica_pool.clone(),
        Arc::new(master_pool_sink),
    )
    .with_sealer(Arc::new(sequencer_sealer));

    let max_concurrency = web3_json_config.vm_concurrency_limit();
    let (vm_concurrency_limiter, vm_barrier) = VmConcurrencyLimiter::new(max_concurrency);

    let batch_fee_input_provider =
        ApiFeeInputProvider::new(batch_fee_model_input_provider, replica_pool);

    let tx_sender = tx_sender_builder.build(
        Arc::new(batch_fee_input_provider),
        Arc::new(vm_concurrency_limiter),
        ApiContracts::load_from_disk().await?,
        storage_caches,
    );
    Ok((tx_sender, vm_barrier))
}

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
    /// Contracts to be used after the allow-list removal upgrade
    pub(crate) post_allowlist_removal: BaseSystemContracts,
    /// Contracts to be used after the 1.4.1 upgrade
    pub(crate) post_1_4_1: BaseSystemContracts,
    /// Contracts to be used after the 1.4.2 upgrade
    pub(crate) post_1_4_2: BaseSystemContracts,
    /// Contracts to be used during the `v23` upgrade. This upgrade was done on an internal staging environment only.
    pub(crate) vm_1_5_0_small_memory: BaseSystemContracts,
    /// Contracts to be used after the 1.5.0 upgrade
    pub(crate) vm_1_5_0_increased_memory: BaseSystemContracts,
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
            ProtocolVersionId::Version18 => self.post_boojum,
            ProtocolVersionId::Version19 => self.post_allowlist_removal,
            ProtocolVersionId::Version20 => self.post_1_4_1,
            ProtocolVersionId::Version21 | ProtocolVersionId::Version22 => self.post_1_4_2,
            ProtocolVersionId::Version23 => self.vm_1_5_0_small_memory,
            ProtocolVersionId::Version24 | ProtocolVersionId::Version25 => {
                self.vm_1_5_0_increased_memory
            }
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
    /// experience (e.g. revert messages).
    pub(crate) eth_call: MultiVMBaseSystemContracts,
}

impl ApiContracts {
    /// Loads the contracts from the local file system.
    /// This method is *currently* preferred to be used in all contexts,
    /// given that there is no way to fetch "playground" contracts from the main node.
    pub async fn load_from_disk() -> anyhow::Result<Self> {
        tokio::task::spawn_blocking(Self::load_from_disk_blocking)
            .await
            .context("loading `ApiContracts` panicked")
    }

    /// Blocking version of [`Self::load_from_disk()`].
    pub fn load_from_disk_blocking() -> Self {
        Self {
            estimate_gas: MultiVMBaseSystemContracts {
                pre_virtual_blocks: BaseSystemContracts::estimate_gas_pre_virtual_blocks(),
                post_virtual_blocks: BaseSystemContracts::estimate_gas_post_virtual_blocks(),
                post_virtual_blocks_finish_upgrade_fix:
                    BaseSystemContracts::estimate_gas_post_virtual_blocks_finish_upgrade_fix(),
                post_boojum: BaseSystemContracts::estimate_gas_post_boojum(),
                post_allowlist_removal: BaseSystemContracts::estimate_gas_post_allowlist_removal(),
                post_1_4_1: BaseSystemContracts::estimate_gas_post_1_4_1(),
                post_1_4_2: BaseSystemContracts::estimate_gas_post_1_4_2(),
                vm_1_5_0_small_memory: BaseSystemContracts::estimate_gas_1_5_0_small_memory(),
                vm_1_5_0_increased_memory:
                    BaseSystemContracts::estimate_gas_post_1_5_0_increased_memory(),
            },
            eth_call: MultiVMBaseSystemContracts {
                pre_virtual_blocks: BaseSystemContracts::playground_pre_virtual_blocks(),
                post_virtual_blocks: BaseSystemContracts::playground_post_virtual_blocks(),
                post_virtual_blocks_finish_upgrade_fix:
                    BaseSystemContracts::playground_post_virtual_blocks_finish_upgrade_fix(),
                post_boojum: BaseSystemContracts::playground_post_boojum(),
                post_allowlist_removal: BaseSystemContracts::playground_post_allowlist_removal(),
                post_1_4_1: BaseSystemContracts::playground_post_1_4_1(),
                post_1_4_2: BaseSystemContracts::playground_post_1_4_2(),
                vm_1_5_0_small_memory: BaseSystemContracts::playground_1_5_0_small_memory(),
                vm_1_5_0_increased_memory:
                    BaseSystemContracts::playground_post_1_5_0_increased_memory(),
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
    replica_connection_pool: ConnectionPool<Core>,
    /// Sink to be used to persist transactions.
    tx_sink: Arc<dyn TxSink>,
    /// Batch sealer used to check whether transaction can be executed by the sequencer.
    sealer: Option<Arc<dyn ConditionalSealer>>,
    /// Cache for tokens that are white-listed for AA.
    whitelisted_tokens_for_aa_cache: Option<Arc<RwLock<Vec<Address>>>>,
}

impl TxSenderBuilder {
    pub fn new(
        config: TxSenderConfig,
        replica_connection_pool: ConnectionPool<Core>,
        tx_sink: Arc<dyn TxSink>,
    ) -> Self {
        Self {
            config,
            replica_connection_pool,
            tx_sink,
            sealer: None,
            whitelisted_tokens_for_aa_cache: None,
        }
    }

    pub fn with_sealer(mut self, sealer: Arc<dyn ConditionalSealer>) -> Self {
        self.sealer = Some(sealer);
        self
    }

    pub fn with_whitelisted_tokens_for_aa(mut self, cache: Arc<RwLock<Vec<Address>>>) -> Self {
        self.whitelisted_tokens_for_aa_cache = Some(cache);
        self
    }

    pub fn build(
        self,
        batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
        vm_concurrency_limiter: Arc<VmConcurrencyLimiter>,
        api_contracts: ApiContracts,
        storage_caches: PostgresStorageCaches,
    ) -> TxSender {
        // Use noop sealer if no sealer was explicitly provided.
        let sealer = self.sealer.unwrap_or_else(|| Arc::new(NoopSealer));
        let whitelisted_tokens_for_aa_cache =
            self.whitelisted_tokens_for_aa_cache.unwrap_or_else(|| {
                Arc::new(RwLock::new(self.config.whitelisted_tokens_for_aa.clone()))
            });
        let missed_storage_invocation_limit = self
            .config
            .vm_execution_cache_misses_limit
            .unwrap_or(usize::MAX);

        TxSender(Arc::new(TxSenderInner {
            sender_config: self.config,
            tx_sink: self.tx_sink,
            replica_connection_pool: self.replica_connection_pool,
            batch_fee_input_provider,
            api_contracts,
            vm_concurrency_limiter,
            storage_caches,
            whitelisted_tokens_for_aa_cache,
            sealer,
            executor: TransactionExecutor::real(missed_storage_invocation_limit),
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
    pub max_allowed_l2_tx_gas_limit: u64,
    pub vm_execution_cache_misses_limit: Option<usize>,
    pub validation_computational_gas_limit: u32,
    pub chain_id: L2ChainId,
    pub whitelisted_tokens_for_aa: Vec<Address>,
}

impl TxSenderConfig {
    pub fn new(
        state_keeper_config: &StateKeeperConfig,
        web3_json_config: &Web3JsonRpcConfig,
        fee_account_addr: Address,
        chain_id: L2ChainId,
    ) -> Self {
        Self {
            fee_account_addr,
            gas_price_scale_factor: web3_json_config.gas_price_scale_factor,
            max_nonce_ahead: web3_json_config.max_nonce_ahead,
            max_allowed_l2_tx_gas_limit: state_keeper_config.max_allowed_l2_tx_gas_limit,
            vm_execution_cache_misses_limit: web3_json_config.vm_execution_cache_misses_limit,
            validation_computational_gas_limit: state_keeper_config
                .validation_computational_gas_limit,
            chain_id,
            whitelisted_tokens_for_aa: web3_json_config.whitelisted_tokens_for_aa.clone(),
        }
    }
}

pub struct TxSenderInner {
    pub(super) sender_config: TxSenderConfig,
    /// Sink to be used to persist transactions.
    pub tx_sink: Arc<dyn TxSink>,
    pub replica_connection_pool: ConnectionPool<Core>,
    // Used to keep track of gas prices for the fee ticker.
    pub batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    pub(super) api_contracts: ApiContracts,
    /// Used to limit the amount of VMs that can be executed simultaneously.
    pub(super) vm_concurrency_limiter: Arc<VmConcurrencyLimiter>,
    // Caches used in VM execution.
    storage_caches: PostgresStorageCaches,
    // Cache for white-listed tokens.
    pub(super) whitelisted_tokens_for_aa_cache: Arc<RwLock<Vec<Address>>>,
    /// Batch sealer used to check whether transaction can be executed by the sequencer.
    pub(super) sealer: Arc<dyn ConditionalSealer>,
    pub(super) executor: TransactionExecutor,
}

#[derive(Clone)]
pub struct TxSender(pub(super) Arc<TxSenderInner>);

impl std::fmt::Debug for TxSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxSender").finish()
    }
}

impl TxSender {
    pub(crate) fn vm_concurrency_limiter(&self) -> Arc<VmConcurrencyLimiter> {
        Arc::clone(&self.0.vm_concurrency_limiter)
    }

    pub(crate) fn storage_caches(&self) -> PostgresStorageCaches {
        self.0.storage_caches.clone()
    }

    pub(crate) async fn read_whitelisted_tokens_for_aa_cache(&self) -> Vec<Address> {
        self.0.whitelisted_tokens_for_aa_cache.read().await.clone()
    }

    async fn acquire_replica_connection(&self) -> anyhow::Result<Connection<'static, Core>> {
        self.0
            .replica_connection_pool
            .connection_tagged("api")
            .await
            .context("failed acquiring connection to replica DB")
    }

    #[tracing::instrument(level = "debug", skip_all, fields(tx.hash = ?tx.hash()))]
    pub async fn submit_tx(
        &self,
        tx: L2Tx,
    ) -> Result<(L2TxSubmissionResult, VmExecutionResultAndLogs), SubmitTxError> {
        let tx_hash = tx.hash();
        let stage_latency = SANDBOX_METRICS.start_tx_submit_stage(tx_hash, SubmitTxStage::Validate);
        let mut connection = self.acquire_replica_connection().await?;
        let protocol_version = connection.blocks_dal().pending_protocol_version().await?;
        drop(connection);
        self.validate_tx(&tx, protocol_version).await?;
        stage_latency.observe();

        let stage_latency = SANDBOX_METRICS.start_tx_submit_stage(tx_hash, SubmitTxStage::DryRun);
        let shared_args = self.call_args(&tx, None).await?;
        let vm_permit = self.0.vm_concurrency_limiter.acquire().await;
        let vm_permit = vm_permit.ok_or(SubmitTxError::ServerShuttingDown)?;
        let mut connection = self.acquire_replica_connection().await?;
        let block_args = BlockArgs::pending(&mut connection).await?;

        let execution_output = self
            .0
            .executor
            .execute_tx_in_sandbox(
                vm_permit.clone(),
                shared_args.clone(),
                TxExecutionArgs::for_validation(tx.clone()),
                connection,
                block_args,
                None,
                vec![],
            )
            .await?;
        tracing::info!(
            "Submit tx {tx_hash:?} with execution metrics {:?}",
            execution_output.metrics
        );
        stage_latency.observe();

        let stage_latency =
            SANDBOX_METRICS.start_tx_submit_stage(tx_hash, SubmitTxStage::VerifyExecute);
        let connection = self.acquire_replica_connection().await?;
        let computational_gas_limit = self.0.sender_config.validation_computational_gas_limit;
        let validation_result = self
            .0
            .executor
            .validate_tx_in_sandbox(
                connection,
                vm_permit,
                tx.clone(),
                shared_args,
                block_args,
                computational_gas_limit,
            )
            .await;
        stage_latency.observe();

        if let Err(err) = validation_result {
            return Err(err.into());
        }
        if !execution_output.are_published_bytecodes_ok {
            return Err(SubmitTxError::FailedToPublishCompressedBytecodes);
        }

        let mut stage_latency =
            SANDBOX_METRICS.start_tx_submit_stage(tx_hash, SubmitTxStage::DbInsert);
        self.ensure_tx_executable(&tx.clone().into(), &execution_output.metrics, true)?;
        let submission_res_handle = self
            .0
            .tx_sink
            .submit_tx(&tx, execution_output.metrics)
            .await?;

        match submission_res_handle {
            L2TxSubmissionResult::AlreadyExecuted => {
                let initiator_account = tx.initiator_account();
                let Nonce(expected_nonce) = self
                    .get_expected_nonce(initiator_account)
                    .await
                    .with_context(|| {
                        format!("failed getting expected nonce for {initiator_account:?}")
                    })?;
                Err(SubmitTxError::NonceIsTooLow(
                    expected_nonce,
                    expected_nonce + self.0.sender_config.max_nonce_ahead,
                    tx.nonce().0,
                ))
            }
            L2TxSubmissionResult::Duplicate => {
                Err(SubmitTxError::IncorrectTx(TxDuplication(tx.hash())))
            }
            L2TxSubmissionResult::InsertionInProgress => Err(SubmitTxError::InsertionInProgress),
            L2TxSubmissionResult::Proxied => {
                stage_latency.set_stage(SubmitTxStage::TxProxy);
                stage_latency.observe();
                Ok((submission_res_handle, execution_output.vm))
            }
            _ => {
                stage_latency.observe();
                Ok((submission_res_handle, execution_output.vm))
            }
        }
    }

    /// **Important.** For the main node, this method acquires a DB connection inside `get_batch_fee_input()`.
    /// Thus, you shouldn't call it if you're holding a DB connection already.
    async fn call_args(
        &self,
        tx: &L2Tx,
        call_overrides: Option<&CallOverrides>,
    ) -> anyhow::Result<TxSetupArgs> {
        let fee_input = self
            .0
            .batch_fee_input_provider
            .get_batch_fee_input()
            .await
            .context("cannot get batch fee input")?;
        Ok(TxSetupArgs {
            execution_mode: if call_overrides.is_some() {
                TxExecutionMode::EthCall
            } else {
                TxExecutionMode::VerifyExecute
            },
            operator_account: AccountTreeId::new(self.0.sender_config.fee_account_addr),
            fee_input,
            base_system_contracts: self.0.api_contracts.eth_call.clone(),
            caches: self.storage_caches(),
            validation_computational_gas_limit: self
                .0
                .sender_config
                .validation_computational_gas_limit,
            chain_id: self.0.sender_config.chain_id,
            whitelisted_tokens_for_aa: self.read_whitelisted_tokens_for_aa_cache().await,
            enforced_base_fee: if let Some(overrides) = call_overrides {
                overrides.enforced_base_fee
            } else {
                Some(tx.common_data.fee.max_fee_per_gas.as_u64())
            },
        })
    }

    async fn validate_tx(
        &self,
        tx: &L2Tx,
        protocol_version: ProtocolVersionId,
    ) -> Result<(), SubmitTxError> {
        // This check is intended to ensure that the gas-related values will be safe to convert to u64 in the future computations.
        let max_gas = U256::from(u64::MAX);
        if tx.common_data.fee.gas_limit > max_gas
            || tx.common_data.fee.gas_per_pubdata_limit > max_gas
        {
            return Err(SubmitTxError::GasLimitIsTooBig);
        }

        let max_allowed_gas_limit = get_max_batch_gas_limit(protocol_version.into());
        if tx.common_data.fee.gas_limit > max_allowed_gas_limit.into() {
            return Err(SubmitTxError::GasLimitIsTooBig);
        }

        let fee_input = self
            .0
            .batch_fee_input_provider
            .get_batch_fee_input()
            .await?;

        // TODO (SMA-1715): do not subsidize the overhead for the transaction

        if tx.common_data.fee.gas_limit > self.0.sender_config.max_allowed_l2_tx_gas_limit.into() {
            tracing::info!(
                "Submitted Tx is Unexecutable {:?} because of GasLimitIsTooBig {}",
                tx.hash(),
                tx.common_data.fee.gas_limit,
            );
            return Err(SubmitTxError::GasLimitIsTooBig);
        }
        if tx.common_data.fee.max_fee_per_gas < fee_input.fair_l2_gas_price().into() {
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
        if tx.execute.factory_deps.len() > MAX_NEW_FACTORY_DEPS {
            return Err(SubmitTxError::TooManyFactoryDependencies(
                tx.execute.factory_deps.len(),
                MAX_NEW_FACTORY_DEPS,
            ));
        }

        let intrinsic_consts = get_intrinsic_constants();
        assert!(
            intrinsic_consts.l2_tx_intrinsic_pubdata == 0,
            "Currently we assume that the L2 transactions do not have any intrinsic pubdata"
        );
        let min_gas_limit = U256::from(intrinsic_consts.l2_tx_intrinsic_gas);
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
        let Nonce(expected_nonce) = self
            .get_expected_nonce(tx.initiator_account())
            .await
            .with_context(|| {
                format!(
                    "failed getting expected nonce for {:?}",
                    tx.initiator_account()
                )
            })?;

        if tx.common_data.nonce.0 < expected_nonce {
            Err(SubmitTxError::NonceIsTooLow(
                expected_nonce,
                expected_nonce + self.0.sender_config.max_nonce_ahead,
                tx.nonce().0,
            ))
        } else {
            let max_nonce = expected_nonce + self.0.sender_config.max_nonce_ahead;
            if !(expected_nonce..=max_nonce).contains(&tx.common_data.nonce.0) {
                Err(SubmitTxError::NonceIsTooHigh(
                    expected_nonce,
                    max_nonce,
                    tx.nonce().0,
                ))
            } else {
                Ok(())
            }
        }
    }

    async fn get_expected_nonce(&self, initiator_account: Address) -> anyhow::Result<Nonce> {
        let mut storage = self.acquire_replica_connection().await?;
        let latest_block_number = storage
            .blocks_dal()
            .get_sealed_l2_block_number()
            .await?
            .context("no L2 blocks in storage")?;

        let nonce = storage
            .storage_web3_dal()
            .get_address_historical_nonce(initiator_account, latest_block_number)
            .await
            .with_context(|| {
                format!("failed getting nonce for address {initiator_account:?} at L2 block #{latest_block_number}")
            })?;
        let nonce = u32::try_from(nonce)
            .map_err(|err| anyhow::anyhow!("failed converting nonce to u32: {err}"))?;
        Ok(Nonce(nonce))
    }

    async fn validate_enough_balance(&self, tx: &L2Tx) -> Result<(), SubmitTxError> {
        let paymaster = tx.common_data.paymaster_params.paymaster;
        // The paymaster is expected to pay for the tx; whatever balance the user has, we don't care.
        if paymaster != Address::default() {
            return Ok(());
        }

        let balance = self.get_balance(&tx.common_data.initiator_address).await?;
        // Estimate the minimum fee price user will agree to.
        let gas_price = tx.common_data.fee.max_fee_per_gas;
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

    async fn get_balance(&self, initiator_address: &H160) -> anyhow::Result<U256> {
        let eth_balance_key = storage_key_for_eth_balance(initiator_address);
        let balance = self
            .acquire_replica_connection()
            .await?
            .storage_web3_dal()
            .get_value(&eth_balance_key)
            .await?;
        Ok(h256_to_u256(balance))
    }

    /// Given the gas_limit to be used for the body of the transaction,
    /// returns the result for executing the transaction with such gas_limit
    #[allow(clippy::too_many_arguments)]
    async fn estimate_gas_step(
        &self,
        vm_permit: VmPermit,
        mut tx: Transaction,
        tx_gas_limit: u64,
        gas_price_per_pubdata: u32,
        fee_model_params: BatchFeeInput,
        block_args: BlockArgs,
        base_fee: u64,
        vm_version: VmVersion,
        state_override: Option<StateOverride>,
    ) -> anyhow::Result<(VmExecutionResultAndLogs, TransactionExecutionMetrics)> {
        let gas_limit_with_overhead = tx_gas_limit
            + derive_overhead(
                tx_gas_limit,
                gas_price_per_pubdata,
                tx.encoding_len(),
                tx.tx_format() as u8,
                vm_version,
            ) as u64;
        // We need to ensure that we never use a gas limit that is higher than the maximum allowed
        let forced_gas_limit = gas_limit_with_overhead.min(get_max_batch_gas_limit(vm_version));

        match &mut tx.common_data {
            ExecuteTransactionCommon::L1(l1_common_data) => {
                l1_common_data.gas_limit = forced_gas_limit.into();
                let required_funds =
                    l1_common_data.gas_limit * l1_common_data.max_fee_per_gas + tx.execute.value;
                l1_common_data.to_mint = required_funds;
            }
            ExecuteTransactionCommon::L2(l2_common_data) => {
                l2_common_data.fee.gas_limit = forced_gas_limit.into();
            }
            ExecuteTransactionCommon::ProtocolUpgrade(common_data) => {
                common_data.gas_limit = forced_gas_limit.into();

                let required_funds =
                    common_data.gas_limit * common_data.max_fee_per_gas + tx.execute.value;

                common_data.to_mint = required_funds;
            }
        }

        let shared_args = self.args_for_gas_estimate(fee_model_params, base_fee).await;
        let execution_args = TxExecutionArgs::for_gas_estimate(tx);
        let connection = self.acquire_replica_connection().await?;
        let execution_output = self
            .0
            .executor
            .execute_tx_in_sandbox(
                vm_permit,
                shared_args,
                execution_args,
                connection,
                block_args,
                state_override,
                vec![],
            )
            .await?;
        Ok((execution_output.vm, execution_output.metrics))
    }

    async fn args_for_gas_estimate(&self, fee_input: BatchFeeInput, base_fee: u64) -> TxSetupArgs {
        let config = &self.0.sender_config;
        TxSetupArgs {
            execution_mode: TxExecutionMode::EstimateFee,
            operator_account: AccountTreeId::new(config.fee_account_addr),
            fee_input,
            // We want to bypass the computation gas limit check for gas estimation
            validation_computational_gas_limit: BATCH_COMPUTATIONAL_GAS_LIMIT,
            base_system_contracts: self.0.api_contracts.estimate_gas.clone(),
            caches: self.storage_caches(),
            chain_id: config.chain_id,
            whitelisted_tokens_for_aa: self.read_whitelisted_tokens_for_aa_cache().await,
            enforced_base_fee: Some(base_fee),
        }
    }

    #[tracing::instrument(level = "debug", skip_all, fields(
        initiator = ?tx.initiator_account(),
        nonce = ?tx.nonce(),
    ))]
    pub async fn get_txs_fee_in_wei(
        &self,
        mut tx: Transaction,
        estimated_fee_scale_factor: f64,
        acceptable_overestimation: u64,
        state_override: Option<StateOverride>,
    ) -> Result<Fee, SubmitTxError> {
        let estimation_started_at = Instant::now();

        let mut connection = self.acquire_replica_connection().await?;
        let block_args = BlockArgs::pending(&mut connection).await?;
        let protocol_version = connection
            .blocks_dal()
            .pending_protocol_version()
            .await
            .context("failed getting pending protocol version")?;
        let max_gas_limit = get_max_batch_gas_limit(protocol_version.into());
        drop(connection);

        let fee_input = adjust_pubdata_price_for_tx(
            self.scaled_batch_fee_input().await?,
            tx.gas_per_pubdata_byte_limit(),
            // We do not have to adjust the params to the `gasPrice` of the transaction, since
            // its gas price will be amended later on to suit the `fee_input`
            None,
            protocol_version.into(),
        );

        let (base_fee, gas_per_pubdata_byte) =
            derive_base_fee_and_gas_per_pubdata(fee_input, protocol_version.into());
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
        // If the default account does not have enough funds for transferring `tx.value`, without taking into account the fee,
        // there is no sense to estimate the fee.
        let account_code_hash = self
            .acquire_replica_connection()
            .await?
            .storage_web3_dal()
            .get_value(&hashed_key)
            .await
            .with_context(|| {
                format!(
                    "failed getting code hash for account {:?}",
                    tx.initiator_account()
                )
            })?;

        if !tx.is_l1() && account_code_hash == H256::zero() {
            let balance = match state_override
                .as_ref()
                .and_then(|overrides| overrides.get(&tx.initiator_account()))
                .and_then(|account| account.balance)
            {
                Some(balance) => balance,
                None => self.get_balance(&tx.initiator_account()).await?,
            };

            if tx.execute.value > balance {
                tracing::info!(
                    "fee estimation failed on validation step.
                    account: {} does not have enough funds for for transferring tx.value: {}.",
                    tx.initiator_account(),
                    tx.execute.value
                );
                return Err(SubmitTxError::InsufficientFundsForTransfer);
            }
        }

        // For L2 transactions we need a properly formatted signature
        if let ExecuteTransactionCommon::L2(l2_common_data) = &mut tx.common_data {
            if l2_common_data.signature.is_empty() {
                l2_common_data.signature = PackedEthSignature::default().serialize_packed().into();
            }
        }

        // Acquire the vm token for the whole duration of the binary search.
        let vm_permit = self.0.vm_concurrency_limiter.acquire().await;
        let vm_permit = vm_permit.ok_or(SubmitTxError::ServerShuttingDown)?;

        // When the pubdata cost grows very high, the total gas limit required may become very high as well. If
        // we do binary search over any possible gas limit naively, we may end up with a very high number of iterations,
        // which affects performance.
        //
        // To optimize for this case, we first calculate the amount of gas needed to cover for the pubdata. After that, we
        // need to do a smaller binary search that is focused on computational gas limit only.
        let additional_gas_for_pubdata = if tx.is_l1() {
            // For L1 transactions the pubdata priced in such a way that the maximal computational
            // gas limit should be enough to cover for the pubdata as well, so no additional gas is provided there.
            0u64
        } else {
            // For L2 transactions, we estimate the amount of gas needed to cover for the pubdata by creating a transaction with infinite gas limit.
            // And getting how much pubdata it used.

            // In theory, if the transaction has failed with such large gas limit, we could have returned an API error here right away,
            // but doing it later on keeps the code more lean.
            let (result, _) = self
                .estimate_gas_step(
                    vm_permit.clone(),
                    tx.clone(),
                    max_gas_limit,
                    gas_per_pubdata_byte as u32,
                    fee_input,
                    block_args,
                    base_fee,
                    protocol_version.into(),
                    state_override.clone(),
                )
                .await
                .context("estimate_gas step failed")?;

            // It is assumed that there is no overflow here
            (result.statistics.pubdata_published as u64) * gas_per_pubdata_byte
        };

        // We are using binary search to find the minimal values of gas_limit under which
        // the transaction succeeds
        let mut lower_bound = 0;
        let mut upper_bound = MAX_L2_TX_GAS_LIMIT;
        tracing::trace!(
            "preparation took {:?}, starting binary search",
            estimation_started_at.elapsed()
        );

        let mut number_of_iterations = 0usize;
        while lower_bound + acceptable_overestimation < upper_bound {
            let mid = (lower_bound + upper_bound) / 2;
            // There is no way to distinct between errors due to out of gas
            // or normal execution errors, so we just hope that increasing the
            // gas limit will make the transaction successful
            let iteration_started_at = Instant::now();
            let try_gas_limit = additional_gas_for_pubdata + mid;
            let (result, _) = self
                .estimate_gas_step(
                    vm_permit.clone(),
                    tx.clone(),
                    try_gas_limit,
                    gas_per_pubdata_byte as u32,
                    fee_input,
                    block_args,
                    base_fee,
                    protocol_version.into(),
                    state_override.clone(),
                )
                .await
                .context("estimate_gas step failed")?;

            if result.result.is_failed() {
                lower_bound = mid + 1;
            } else {
                upper_bound = mid;
            }

            tracing::trace!(
                "iteration {number_of_iterations} took {:?}. lower_bound: {lower_bound}, upper_bound: {upper_bound}",
                iteration_started_at.elapsed()
            );
            number_of_iterations += 1;
        }
        SANDBOX_METRICS
            .estimate_gas_binary_search_iterations
            .observe(number_of_iterations);

        let suggested_gas_limit =
            ((upper_bound + additional_gas_for_pubdata) as f64 * estimated_fee_scale_factor) as u64;
        let (result, tx_metrics) = self
            .estimate_gas_step(
                vm_permit,
                tx.clone(),
                suggested_gas_limit,
                gas_per_pubdata_byte as u32,
                fee_input,
                block_args,
                base_fee,
                protocol_version.into(),
                state_override,
            )
            .await
            .context("final estimate_gas step failed")?;

        result.into_api_call_result()?;
        self.ensure_tx_executable(&tx, &tx_metrics, false)?;

        // Now, we need to calculate the final overhead for the transaction.
        let overhead = derive_overhead(
            suggested_gas_limit,
            gas_per_pubdata_byte as u32,
            tx.encoding_len(),
            tx.tx_format() as u8,
            protocol_version.into(),
        ) as u64;

        let full_gas_limit = match suggested_gas_limit.overflowing_add(overhead) {
            (value, false) => {
                if value > max_gas_limit {
                    return Err(SubmitTxError::ExecutionReverted(
                        "exceeds block gas limit".to_string(),
                        vec![],
                    ));
                }

                value
            }
            (_, true) => {
                return Err(SubmitTxError::ExecutionReverted(
                    "exceeds block gas limit".to_string(),
                    vec![],
                ));
            }
        };

        let gas_for_pubdata = (tx_metrics.pubdata_published as u64) * gas_per_pubdata_byte;
        let estimated_gas_for_pubdata =
            (gas_for_pubdata as f64 * estimated_fee_scale_factor) as u64;

        tracing::debug!(
            "gas for pubdata: {estimated_gas_for_pubdata}, computational gas: {}, overhead gas: {overhead} \
            (with params base_fee: {base_fee}, gas_per_pubdata_byte: {gas_per_pubdata_byte}) \
            estimated_fee_scale_factor: {estimated_fee_scale_factor}",
            suggested_gas_limit - estimated_gas_for_pubdata,
        );

        Ok(Fee {
            max_fee_per_gas: base_fee.into(),
            max_priority_fee_per_gas: 0u32.into(),
            gas_limit: full_gas_limit.into(),
            gas_per_pubdata_limit: gas_per_pubdata_byte.into(),
        })
    }

    // For now, both L1 gas price and pubdata price are scaled with the same coefficient
    async fn scaled_batch_fee_input(&self) -> anyhow::Result<BatchFeeInput> {
        self.0
            .batch_fee_input_provider
            .get_batch_fee_input_scaled(
                self.0.sender_config.gas_price_scale_factor,
                self.0.sender_config.gas_price_scale_factor,
            )
            .await
    }

    pub(super) async fn eth_call(
        &self,
        block_args: BlockArgs,
        call_overrides: CallOverrides,
        tx: L2Tx,
        state_override: Option<StateOverride>,
    ) -> Result<Vec<u8>, SubmitTxError> {
        let vm_permit = self.0.vm_concurrency_limiter.acquire().await;
        let vm_permit = vm_permit.ok_or(SubmitTxError::ServerShuttingDown)?;

        let connection = self.acquire_replica_connection().await?;
        let result = self
            .0
            .executor
            .execute_tx_in_sandbox(
                vm_permit,
                self.call_args(&tx, Some(&call_overrides)).await?,
                TxExecutionArgs::for_eth_call(tx),
                connection,
                block_args,
                state_override,
                vec![],
            )
            .await?;
        result.vm.into_api_call_result()
    }

    pub async fn gas_price(&self) -> anyhow::Result<u64> {
        let mut connection = self.acquire_replica_connection().await?;
        let protocol_version = connection
            .blocks_dal()
            .pending_protocol_version()
            .await
            .context("failed obtaining pending protocol version")?;
        drop(connection);

        let (base_fee, _) = derive_base_fee_and_gas_per_pubdata(
            self.scaled_batch_fee_input().await?,
            protocol_version.into(),
        );
        Ok(base_fee)
    }

    fn ensure_tx_executable(
        &self,
        transaction: &Transaction,
        tx_metrics: &TransactionExecutionMetrics,
        log_message: bool,
    ) -> Result<(), SubmitTxError> {
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
        if let Some(reason) = self
            .0
            .sealer
            .find_unexecutable_reason(&seal_data, protocol_version)
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

    // FIXME: rework as `BlockArgs` method with supplied connection
    pub(crate) async fn get_default_eth_call_gas(
        &self,
        block_args: BlockArgs,
    ) -> anyhow::Result<u64> {
        let mut connection = self.acquire_replica_connection().await?;

        let protocol_version = block_args
            .resolve_block_info(&mut connection)
            .await
            .context("failed to resolve block info")?
            .protocol_version;

        Ok(get_eth_call_gas_limit(protocol_version.into()))
    }
}
