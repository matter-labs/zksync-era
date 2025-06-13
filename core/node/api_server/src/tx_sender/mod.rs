//! Helper module to submit transactions into the ZKsync Network.

use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::Context as _;
use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::RwLock;
use zksync_config::configs::{api::Web3JsonRpcConfig, chain::StateKeeperConfig};
use zksync_dal::{
    transactions_dal::L2TxSubmissionResult, Connection, ConnectionPool, Core, CoreDal,
};
use zksync_health_check::{CheckHealth, Health, HealthStatus};
use zksync_multivm::{
    interface::{
        tracer::TimestampAsserterParams as TracerTimestampAsserterParams, OneshotTracingParams,
        TransactionExecutionMetrics,
    },
    utils::{
        derive_base_fee_and_gas_per_pubdata, get_max_batch_gas_limit, get_max_new_factory_deps,
    },
};
use zksync_node_fee_model::{ApiFeeInputProvider, BatchFeeModelInputProvider};
use zksync_object_store::ObjectStore;
use zksync_state::PostgresStorageCaches;
use zksync_types::{
    api::state_override::StateOverride,
    fee_model::BatchFeeInput,
    get_intrinsic_constants, h256_to_u256,
    l2::{error::TxCheckError::TxDuplication, L2Tx},
    transaction_request::CallOverrides,
    utils::storage_key_for_eth_balance,
    vm::FastVmMode,
    AccountTreeId, Address, L2ChainId, Nonce, ProtocolVersionId, Transaction, H160, H256, U256,
};
use zksync_vm_executor::{
    interface::TransactionFilter,
    oneshot::{CallOrExecute, EstimateGas, MultiVmBaseSystemContracts, OneshotEnvParameters},
};

pub(super) use self::{gas_estimation::BinarySearchKind, result::SubmitTxError};
use self::{master_pool_sink::MasterPoolSink, result::ApiCallResult, tx_sink::TxSink};
use crate::execution_sandbox::{
    BlockArgs, SandboxAction, SandboxExecutionOutput, SandboxExecutor, SubmitTxStage,
    VmConcurrencyBarrier, VmConcurrencyLimiter, SANDBOX_METRICS,
};

mod gas_estimation;
pub mod master_pool_sink;
pub mod proxy;
mod result;
#[cfg(test)]
pub(crate) mod tests;
pub mod tx_sink;
pub mod whitelist;

pub async fn build_tx_sender(
    tx_sender_config: &TxSenderConfig,
    web3_json_config: &Web3JsonRpcConfig,
    replica_pool: ConnectionPool<Core>,
    master_pool: ConnectionPool<Core>,
    batch_fee_model_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    storage_caches: PostgresStorageCaches,
) -> anyhow::Result<(TxSender, VmConcurrencyBarrier)> {
    let master_pool_sink = MasterPoolSink::new(master_pool);
    let tx_sender_builder = TxSenderBuilder::new(
        tx_sender_config.clone(),
        replica_pool.clone(),
        Arc::new(master_pool_sink),
    );

    let max_concurrency = web3_json_config.vm_concurrency_limit;
    let (vm_concurrency_limiter, vm_barrier) = VmConcurrencyLimiter::new(max_concurrency);

    let batch_fee_input_provider = ApiFeeInputProvider::new(
        batch_fee_model_input_provider,
        replica_pool,
        web3_json_config.gas_price_scale_factor_open_batch,
    );
    let executor_options = SandboxExecutorOptions::new(
        tx_sender_config.chain_id,
        AccountTreeId::new(tx_sender_config.fee_account_addr),
        tx_sender_config.validation_computational_gas_limit,
    )
    .await?;
    let tx_sender = tx_sender_builder.build(
        Arc::new(batch_fee_input_provider),
        Arc::new(vm_concurrency_limiter),
        executor_options,
        storage_caches,
    );
    Ok((tx_sender, vm_barrier))
}

/// Oneshot executor options used by the API server sandbox.
#[derive(Debug)]
pub struct SandboxExecutorOptions {
    pub(crate) fast_vm_mode: FastVmMode,
    pub(crate) vm_dump_store: Option<Arc<dyn ObjectStore>>,
    /// Env parameters to be used when estimating gas.
    pub(crate) estimate_gas: OneshotEnvParameters<EstimateGas>,
    /// Env parameters to be used when performing `eth_call` requests.
    pub(crate) eth_call: OneshotEnvParameters<CallOrExecute>,
    pub(crate) interrupted_execution_latency_histogram: &'static vise::Histogram<Duration>,
    #[cfg(test)]
    pub(crate) storage_delay: Option<Duration>,
}

impl SandboxExecutorOptions {
    /// Loads the contracts from the local file system.
    /// This method is *currently* preferred to be used in all contexts,
    /// given that there is no way to fetch "playground" contracts from the main node.
    pub async fn new(
        chain_id: L2ChainId,
        operator_account: AccountTreeId,
        validation_computational_gas_limit: u32,
    ) -> anyhow::Result<Self> {
        let estimate_gas_contracts =
            tokio::task::spawn_blocking(MultiVmBaseSystemContracts::load_estimate_gas_blocking)
                .await
                .context("failed loading base contracts for gas estimation")?;
        let call_contracts =
            tokio::task::spawn_blocking(MultiVmBaseSystemContracts::load_eth_call_blocking)
                .await
                .context("failed loading base contracts for calls / tx execution")?;

        Ok(Self {
            fast_vm_mode: FastVmMode::Old,
            vm_dump_store: None,
            estimate_gas: OneshotEnvParameters::new(
                Arc::new(estimate_gas_contracts),
                chain_id,
                operator_account,
                u32::MAX,
            ),
            eth_call: OneshotEnvParameters::new(
                Arc::new(call_contracts),
                chain_id,
                operator_account,
                validation_computational_gas_limit,
            ),
            interrupted_execution_latency_histogram: &SANDBOX_METRICS
                .sandbox_interrupted_execution_latency,
            #[cfg(test)]
            storage_delay: None,
        })
    }

    /// Sets the fast VM mode used by this executor.
    pub fn set_fast_vm_mode(&mut self, fast_vm_mode: FastVmMode) {
        self.fast_vm_mode = fast_vm_mode;
    }

    pub fn set_vm_dump_object_store(&mut self, store: Arc<dyn ObjectStore>) {
        self.vm_dump_store = Some(store);
    }

    pub(crate) async fn mock() -> Self {
        Self::new(L2ChainId::default(), AccountTreeId::default(), u32::MAX)
            .await
            .unwrap()
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
    /// Transaction filter that can be used to reject transactions.
    transaction_filter: Option<Arc<dyn TransactionFilter>>,
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
            transaction_filter: None,
            whitelisted_tokens_for_aa_cache: None,
        }
    }

    pub fn with_transaction_filter(mut self, filter: Arc<dyn TransactionFilter>) -> Self {
        self.transaction_filter = Some(filter);
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
        executor_options: SandboxExecutorOptions,
        storage_caches: PostgresStorageCaches,
    ) -> TxSender {
        // Use noop sealer if no sealer was explicitly provided.
        let transaction_filter = self.transaction_filter.unwrap_or_else(|| Arc::new(()));
        let whitelisted_tokens_for_aa_cache =
            self.whitelisted_tokens_for_aa_cache.unwrap_or_else(|| {
                Arc::new(RwLock::new(self.config.whitelisted_tokens_for_aa.clone()))
            });
        let missed_storage_invocation_limit = self
            .config
            .vm_execution_cache_misses_limit
            .unwrap_or(usize::MAX);
        let executor = SandboxExecutor::real(
            executor_options,
            storage_caches,
            missed_storage_invocation_limit,
            self.config.timestamp_asserter_params.clone().map(|params| {
                TracerTimestampAsserterParams {
                    address: params.address,
                    min_time_till_end: params.min_time_till_end,
                }
            }),
        );

        TxSender(Arc::new(TxSenderInner {
            sender_config: self.config,
            tx_sink: self.tx_sink,
            replica_connection_pool: self.replica_connection_pool,
            batch_fee_input_provider,
            vm_concurrency_limiter,
            whitelisted_tokens_for_aa_cache,
            transaction_filter,
            executor,
        }))
    }
}

/// Internal static `TxSender` configuration.
///
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
    pub timestamp_asserter_params: Option<TimestampAsserterParams>,
}

#[derive(Debug, Clone)]
pub struct TimestampAsserterParams {
    pub address: Address,
    pub min_time_till_end: Duration,
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
            timestamp_asserter_params: None,
        }
    }

    pub fn with_timestamp_asserter_params(
        mut self,
        timestamp_asserter_params: TimestampAsserterParams,
    ) -> Self {
        self.timestamp_asserter_params = Some(timestamp_asserter_params);
        self
    }
}

pub struct TxSenderInner {
    pub(super) sender_config: TxSenderConfig,
    /// Sink to be used to persist transactions.
    pub tx_sink: Arc<dyn TxSink>,
    pub replica_connection_pool: ConnectionPool<Core>,
    // Used to keep track of gas prices for the fee ticker.
    pub batch_fee_input_provider: Arc<dyn BatchFeeModelInputProvider>,
    /// Used to limit the amount of VMs that can be executed simultaneously.
    pub(super) vm_concurrency_limiter: Arc<VmConcurrencyLimiter>,
    // Cache for white-listed tokens.
    pub(super) whitelisted_tokens_for_aa_cache: Arc<RwLock<Vec<Address>>>,
    /// Batch sealer used to check whether transaction can be executed by the sequencer.
    pub(super) transaction_filter: Arc<dyn TransactionFilter>,
    pub(super) executor: SandboxExecutor,
}

/// Health check details for [`TxSender`].
#[derive(Debug, Serialize)]
struct TxSenderHealthDetails {
    vm_mode: FastVmMode,
    #[serde(skip_serializing_if = "TxSenderHealthDetails::is_zero")]
    vm_divergences: usize,
}

impl TxSenderHealthDetails {
    fn is_zero(value: &usize) -> bool {
        *value == 0
    }
}

impl From<TxSenderHealthDetails> for Health {
    fn from(details: TxSenderHealthDetails) -> Self {
        let status = if details.vm_divergences > 0 {
            HealthStatus::Affected
        } else {
            HealthStatus::Ready
        };
        Health::from(status).with_details(details)
    }
}

/// Health check for [`TxSender`].
#[derive(Debug)]
struct TxSenderHealthCheck {
    vm_mode: FastVmMode,
    vm_divergence_counter: Arc<AtomicUsize>,
}

#[async_trait]
impl CheckHealth for TxSenderHealthCheck {
    fn name(&self) -> &'static str {
        "tx_sender"
    }

    async fn check_health(&self) -> Health {
        let details = TxSenderHealthDetails {
            vm_mode: self.vm_mode,
            vm_divergences: self.vm_divergence_counter.load(Ordering::Relaxed),
        };
        details.into()
    }
}

#[derive(Clone)]
pub struct TxSender(pub(super) Arc<TxSenderInner>);

impl std::fmt::Debug for TxSender {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TxSender").finish()
    }
}

impl TxSender {
    pub fn health_check(&self) -> impl CheckHealth {
        TxSenderHealthCheck {
            vm_mode: self.0.executor.vm_mode(),
            vm_divergence_counter: self.0.executor.vm_divergence_counter(),
        }
    }

    pub(crate) fn vm_concurrency_limiter(&self) -> Arc<VmConcurrencyLimiter> {
        Arc::clone(&self.0.vm_concurrency_limiter)
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

    #[tracing::instrument(level = "debug", name = "submit_tx", skip_all, fields(tx.hash = ?tx.hash()))]
    pub(crate) async fn submit_tx(
        &self,
        tx: L2Tx,
        block_args: BlockArgs,
    ) -> Result<SandboxExecutionOutput, SubmitTxError> {
        let tx_hash = tx.hash();
        let stage_latency = SANDBOX_METRICS.start_tx_submit_stage(tx_hash, SubmitTxStage::Validate);
        self.validate_tx(&tx, block_args.protocol_version()).await?;
        stage_latency.observe();

        let stage_latency = SANDBOX_METRICS.start_tx_submit_stage(tx_hash, SubmitTxStage::DryRun);
        // **Important.** For the main node, this method acquires a DB connection inside `get_batch_fee_input()`.
        // Thus, it must not be called it if you're holding a DB connection already.
        let fee_input = self
            .0
            .batch_fee_input_provider
            .get_batch_fee_input()
            .await
            .context("cannot get batch fee input")?;

        let vm_permit = self.0.vm_concurrency_limiter.acquire().await;
        let action = SandboxAction::Execution {
            fee_input,
            tx: tx.clone(),
        };
        let vm_permit = vm_permit.ok_or(SubmitTxError::ServerShuttingDown)?;
        let connection = self.acquire_replica_connection().await?;
        let execution_output = self
            .0
            .executor
            .execute_in_sandbox(vm_permit.clone(), connection, action, &block_args, None)
            .await?;
        tracing::info!(
            "Submit tx {tx_hash:?} with execution metrics {:?}",
            execution_output.metrics
        );
        stage_latency.observe();

        let stage_latency =
            SANDBOX_METRICS.start_tx_submit_stage(tx_hash, SubmitTxStage::VerifyExecute);
        let connection = self.acquire_replica_connection().await?;
        let validation_result = self
            .0
            .executor
            .validate_tx_in_sandbox(
                vm_permit,
                connection,
                tx.clone(),
                block_args,
                fee_input,
                &self.read_whitelisted_tokens_for_aa_cache().await,
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
        self.ensure_tx_executable(&tx.clone().into(), execution_output.metrics, true)
            .await?;

        let validation_traces = validation_result?;
        let submission_res_handle = self
            .0
            .tx_sink
            .submit_tx(&tx, &execution_output, validation_traces)
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
                Ok(execution_output)
            }
            L2TxSubmissionResult::Added | L2TxSubmissionResult::Replaced => {
                stage_latency.observe();
                Ok(execution_output)
            }
        }
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

        // At the moment fair_l2_gas_price is rarely changed for ETH-based chains. But for CBT
        // chains it gets changed every few blocks because of token price change. We want to avoid
        // situations when transactions with low gas price gets into mempool and sit there for a
        // long time, so we require max_fee_per_gas to be at least current_l2_fair_gas_price / 2
        if tx.common_data.fee.max_fee_per_gas < (fee_input.fair_l2_gas_price() / 2).into() {
            tracing::info!(
                "Submitted Tx is Unexecutable {:?} because of MaxFeePerGasTooLow {}",
                tx.hash(),
                tx.common_data.fee.max_fee_per_gas,
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
        let max_new_factory_deps = get_max_new_factory_deps(protocol_version.into());
        if tx.execute.factory_deps.len() > max_new_factory_deps {
            return Err(SubmitTxError::TooManyFactoryDependencies(
                tx.execute.factory_deps.len(),
                max_new_factory_deps,
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

    // For now, both L1 gas price and pubdata price are scaled with the same coefficient
    pub(crate) async fn scaled_batch_fee_input(&self) -> anyhow::Result<BatchFeeInput> {
        self.0
            .batch_fee_input_provider
            .get_batch_fee_input_scaled(
                self.0.sender_config.gas_price_scale_factor,
                self.0.sender_config.gas_price_scale_factor,
            )
            .await
    }

    pub(crate) async fn eth_call(
        &self,
        block_args: BlockArgs,
        call_overrides: CallOverrides,
        call: L2Tx,
        state_override: Option<StateOverride>,
    ) -> Result<Vec<u8>, SubmitTxError> {
        let vm_permit = self.0.vm_concurrency_limiter.acquire().await;
        let vm_permit = vm_permit.ok_or(SubmitTxError::ServerShuttingDown)?;

        let mut connection;
        let fee_input = if block_args.resolves_to_latest_sealed_l2_block() {
            let fee_input = self
                .0
                .batch_fee_input_provider
                .get_batch_fee_input()
                .await?;
            // It is important to acquire a connection after calling the provider; see the comment above.
            connection = self.acquire_replica_connection().await?;
            fee_input
        } else {
            connection = self.acquire_replica_connection().await?;
            block_args.historical_fee_input(&mut connection).await?
        };

        let action = SandboxAction::Call {
            call,
            fee_input,
            enforced_base_fee: call_overrides.enforced_base_fee,
            tracing_params: OneshotTracingParams::default(),
        };
        let result = self
            .0
            .executor
            .execute_in_sandbox(vm_permit, connection, action, &block_args, state_override)
            .await?;
        result.result.into_api_call_result()
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

    async fn ensure_tx_executable(
        &self,
        transaction: &Transaction,
        tx_metrics: TransactionExecutionMetrics,
        log_message: bool,
    ) -> Result<(), SubmitTxError> {
        // Hash is not computable for the provided `transaction` during gas estimation (it doesn't have
        // its input data set). Since we don't log a hash in this case anyway, we just use a dummy value.
        let tx_hash = if log_message {
            transaction.hash()
        } else {
            H256::zero()
        };

        if let Err(reason) = self
            .0
            .transaction_filter
            .filter_transaction(transaction, &tx_metrics)
            .await
        {
            let message = format!(
                "Tx is Unexecutable because of {reason}; inputs for decision: {tx_metrics:?}"
            );
            if log_message {
                tracing::info!("{tx_hash:#?} {message}");
            }
            return Err(SubmitTxError::Unexecutable(message));
        }
        Ok(())
    }
}
