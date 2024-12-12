//! Implementation of "executing" methods, e.g. `eth_call`.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::Context as _;
use async_trait::async_trait;
use ruint::aliases::U256;
use tokio::runtime::Handle;
use tokio::task::spawn_blocking;
use zk_ee::system::system_trait::errors::InternalError;
use zk_os_forward_system::run::{BatchContext, StorageCommitment, TxOutput, ExecutionOutput, ExecutionResult};
use zksync_dal::{Connection, Core};
use zksync_multivm::interface::{executor::{OneshotExecutor, TransactionValidator}, storage::{ReadStorage, StorageWithOverrides}, tracer::{TimestampAsserterParams, ValidationError, ValidationParams, ValidationTraces}, Call, OneshotEnv, OneshotTracingParams, OneshotTransactionExecutionResult, TransactionExecutionMetrics, TxExecutionArgs, VmExecutionResultAndLogs};
use zksync_state::{PostgresStorage, PostgresStorageCaches, PostgresStorageForZkOs};
use zksync_types::{
    api::state_override::StateOverride, fee_model::BatchFeeInput, l2::L2Tx, Transaction,
};
use zksync_vm_executor::oneshot::{MainOneshotExecutor, MockOneshotExecutor};
use zksync_zkos_vm_runner::zkos_conversions::tx_abi_encode;
use super::{
    vm_metrics::{self, SandboxStage},
    BlockArgs, VmPermit, SANDBOX_METRICS,
};
use crate::{execution_sandbox::storage::apply_state_override, tx_sender::SandboxExecutorOptions};

/// Action that can be executed by [`SandboxExecutor`].
#[derive(Debug)]
pub(crate) enum SandboxAction {
    /// Execute a transaction.
    Execution { tx: L2Tx, fee_input: BatchFeeInput },
    /// Execute a call, possibly with tracing.
    Call {
        call: L2Tx,
        fee_input: BatchFeeInput,
        enforced_base_fee: Option<u64>,
        tracing_params: OneshotTracingParams,
    },
    /// Estimate gas for a transaction.
    GasEstimation {
        tx: Transaction,
        fee_input: BatchFeeInput,
        base_fee: u64,
    },
}

impl SandboxAction {
    fn factory_deps_count(&self) -> usize {
        match self {
            Self::Execution { tx, .. } | Self::Call { call: tx, .. } => {
                tx.execute.factory_deps.len()
            }
            Self::GasEstimation { tx, .. } => tx.execute.factory_deps.len(),
        }
    }

    fn into_parts(self) -> (TxExecutionArgs, OneshotTracingParams) {
        match self {
            Self::Execution { tx, .. } => (
                TxExecutionArgs::for_validation(tx),
                OneshotTracingParams::default(),
            ),
            Self::GasEstimation { tx, .. } => (
                TxExecutionArgs::for_gas_estimate(tx),
                OneshotTracingParams::default(),
            ),
            Self::Call {
                call,
                tracing_params,
                ..
            } => (TxExecutionArgs::for_eth_call(call), tracing_params),
        }
    }
}

/// Output of [`SandboxExecutor::execute_in_sandbox()`].
#[derive(Debug, Clone)]
pub(crate) struct SandboxExecutionOutput {
    /// Output of the VM.
    pub vm: VmExecutionResultAndLogs,
    /// Traced calls if requested.
    pub call_traces: Vec<Call>,
    /// Execution metrics.
    pub metrics: TransactionExecutionMetrics,
    /// Were published bytecodes OK?
    pub are_published_bytecodes_ok: bool,
}

#[derive(Debug)]
enum SandboxExecutorEngine {
    Real(MainOneshotExecutor),
    Mock(MockOneshotExecutor),
}

/// Executor of transactions / calls used in the API server.
#[derive(Debug)]
pub(crate) struct SandboxExecutor {
    engine: SandboxExecutorEngine,
    pub(super) options: SandboxExecutorOptions,
    storage_caches: Option<PostgresStorageCaches>,
    pub(super) timestamp_asserter_params: Option<TimestampAsserterParams>,
}

impl SandboxExecutor {
    pub fn real(
        options: SandboxExecutorOptions,
        caches: PostgresStorageCaches,
        missed_storage_invocation_limit: usize,
        timestamp_asserter_params: Option<TimestampAsserterParams>,
    ) -> Self {
        let mut executor = MainOneshotExecutor::new(missed_storage_invocation_limit);
        executor.set_fast_vm_mode(options.fast_vm_mode);
        #[cfg(test)]
        executor.panic_on_divergence();
        executor
            .set_execution_latency_histogram(&SANDBOX_METRICS.sandbox[&SandboxStage::Execution]);
        Self {
            engine: SandboxExecutorEngine::Real(executor),
            options,
            storage_caches: Some(caches),
            timestamp_asserter_params,
        }
    }

    pub(crate) async fn mock(executor: MockOneshotExecutor) -> Self {
        Self::custom_mock(executor, SandboxExecutorOptions::mock().await)
    }

    pub(crate) fn custom_mock(
        executor: MockOneshotExecutor,
        options: SandboxExecutorOptions,
    ) -> Self {
        Self {
            engine: SandboxExecutorEngine::Mock(executor),
            options,
            storage_caches: None,
            timestamp_asserter_params: None,
        }
    }

    pub async fn execute_in_sandbox_zkos(
        &self,
        vm_permit: VmPermit,
        connection: Connection<'static, Core>,
        action: SandboxAction,
        block_args: &BlockArgs,
        state_override: Option<StateOverride>,
    ) -> anyhow::Result<Vec<u8>> {
        let (env, storage) = self
            .prepare_env_and_storage(connection, block_args, &action)
            .await?;

        let (execution_args, tracing_params) = action.into_parts();

        // todo: gas
        let context = BatchContext {
            eip1559_basefee: U256::from(0),
            ergs_price: U256::from(1),
            gas_per_pubdata: Default::default(),
            block_number: (env.l1_batch.number.0 + 1) as u64,
            timestamp: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("Incorrect system time")
                .as_secs(),
        };

        // todo: storage commitment shouldn't be needed here
        let storage_commitment = StorageCommitment {
            root: Default::default(),
            next_free_slot: 0,
        };

        let abi = tx_abi_encode(execution_args.transaction);

        let result =
            spawn_blocking(move || {
                let zkos_storage = PostgresStorageForZkOs::new(storage);
                zk_os_forward_system::run::simulate_tx(
                    abi,
                    storage_commitment,
                    context,
                    // we pass the storage source and preimage source separately,
                    // but then need to backed by the same connection
                    zkos_storage.clone(),
                    zkos_storage,
                )
            }
            ).await
                .expect("");

        tracing::info!("execute_in_sandbox Result: {:?}", result);
        drop(vm_permit);

        //todo: eth_call error format - should be compatible with era/ethereum
        match result {
            Ok(Ok(tx_output)) => {
                match tx_output.execution_result {
                    ExecutionResult::Success(ExecutionOutput::Call(data)) => Ok(data),
                    ExecutionResult::Success(ExecutionOutput::Create(data, _)) => Ok(data),
                    ExecutionResult::Revert(res) => anyhow::bail!("revert: {:?}", res)
                }
            }
            Ok(Err(invalid)) => {
                anyhow::bail!("invalid transaction: {:?}", invalid)
            }
            Err(err) => {
                anyhow::bail!("Execution failed: {:?}", err)
            }
        }
    }

    /// This method assumes that (block with number `resolved_block_number` is present in DB)
    /// or (`block_id` is `pending` and block with number `resolved_block_number - 1` is present in DB)
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn execute_in_sandbox(
        &self,
        vm_permit: VmPermit,
        connection: Connection<'static, Core>,
        action: SandboxAction,
        block_args: &BlockArgs,
        state_override: Option<StateOverride>,
    ) -> anyhow::Result<SandboxExecutionOutput> {
        let total_factory_deps = action.factory_deps_count() as u16;
        let (env, storage) = self
            .prepare_env_and_storage(connection, block_args, &action)
            .await?;

        let state_override = state_override.unwrap_or_default();
        let storage = apply_state_override(storage, &state_override);
        let (execution_args, tracing_params) = action.into_parts();
        let result = self
            .inspect_transaction_with_bytecode_compression(
                storage,
                env,
                execution_args,
                tracing_params,
            )
            .await?;
        drop(vm_permit);

        let metrics =
            vm_metrics::collect_tx_execution_metrics(total_factory_deps, &result.tx_result);
        Ok(SandboxExecutionOutput {
            vm: *result.tx_result,
            call_traces: result.call_traces,
            metrics,
            are_published_bytecodes_ok: result.compression_result.is_ok(),
        })
    }

    pub(super) async fn prepare_env_and_storage(
        &self,
        mut connection: Connection<'static, Core>,
        block_args: &BlockArgs,
        action: &SandboxAction,
    ) -> anyhow::Result<(OneshotEnv, PostgresStorage<'static>)> {
        let initialization_stage = SANDBOX_METRICS.sandbox[&SandboxStage::Initialization].start();
        let resolve_started_at = Instant::now();
        let resolve_time = resolve_started_at.elapsed();
        let resolved_block_info = &block_args.resolved;
        // We don't want to emit too many logs.
        if resolve_time > Duration::from_millis(10) {
            tracing::debug!("Resolved block numbers (took {resolve_time:?})");
        }

        let env = match action {
            SandboxAction::Execution { fee_input, tx } => {
                self.options
                    .eth_call
                    .to_execute_env(&mut connection, resolved_block_info, *fee_input, tx)
                    .await?
            }
            &SandboxAction::Call {
                fee_input,
                enforced_base_fee,
                ..
            } => {
                self.options
                    .eth_call
                    .to_call_env(
                        &mut connection,
                        resolved_block_info,
                        fee_input,
                        enforced_base_fee,
                    )
                    .await?
            }
            &SandboxAction::GasEstimation {
                fee_input,
                base_fee,
                ..
            } => {
                self.options
                    .estimate_gas
                    .to_env(&mut connection, resolved_block_info, fee_input, base_fee)
                    .await?
            }
        };

        if block_args.resolves_to_latest_sealed_l2_block() {
            if let Some(caches) = &self.storage_caches {
                caches.schedule_values_update(resolved_block_info.state_l2_block_number());
            }
        }

        let mut storage = PostgresStorage::new_async(
            Handle::current(),
            connection,
            resolved_block_info.state_l2_block_number(),
            false,
        )
            .await
            .context("cannot create `PostgresStorage`")?;

        if let Some(caches) = &self.storage_caches {
            storage = storage.with_caches(caches.clone());
        }
        initialization_stage.observe();
        Ok((env, storage))
    }
}

#[async_trait]
impl<S> OneshotExecutor<StorageWithOverrides<S>> for SandboxExecutor
where
    S: ReadStorage + Send + 'static,
{
    async fn inspect_transaction_with_bytecode_compression(
        &self,
        storage: StorageWithOverrides<S>,
        env: OneshotEnv,
        args: TxExecutionArgs,
        tracing_params: OneshotTracingParams,
    ) -> anyhow::Result<OneshotTransactionExecutionResult> {
        match &self.engine {
            SandboxExecutorEngine::Real(executor) => {
                executor
                    .inspect_transaction_with_bytecode_compression(
                        storage,
                        env,
                        args,
                        tracing_params,
                    )
                    .await
            }
            SandboxExecutorEngine::Mock(executor) => {
                executor
                    .inspect_transaction_with_bytecode_compression(
                        storage,
                        env,
                        args,
                        tracing_params,
                    )
                    .await
            }
        }
    }
}

#[async_trait]
impl<S> TransactionValidator<StorageWithOverrides<S>> for SandboxExecutor
where
    S: ReadStorage + Send + 'static,
{
    async fn validate_transaction(
        &self,
        storage: StorageWithOverrides<S>,
        env: OneshotEnv,
        tx: L2Tx,
        validation_params: ValidationParams,
    ) -> anyhow::Result<Result<ValidationTraces, ValidationError>> {
        match &self.engine {
            SandboxExecutorEngine::Real(executor) => {
                executor
                    .validate_transaction(storage, env, tx, validation_params)
                    .await
            }
            SandboxExecutorEngine::Mock(executor) => {
                executor
                    .validate_transaction(storage, env, tx, validation_params)
                    .await
            }
        }
    }
}
