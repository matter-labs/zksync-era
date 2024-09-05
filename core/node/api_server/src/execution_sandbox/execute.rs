//! Implementation of "executing" methods, e.g. `eth_call`.

use async_trait::async_trait;
use zksync_dal::{Connection, Core};
use zksync_multivm::interface::{
    executor::{OneshotExecutor, TransactionValidator},
    storage::ReadStorage,
    tracer::{ValidationError, ValidationParams},
    Call, OneshotEnv, OneshotTracingParams, OneshotTransactionExecutionResult,
    TransactionExecutionMetrics, TxExecutionArgs, VmExecutionResultAndLogs,
};
use zksync_types::{api::state_override::StateOverride, l2::L2Tx};
use zksync_vm_executor::oneshot::{MainOneshotExecutor, MockOneshotExecutor};

use super::{
    apply, storage::StorageWithOverrides, vm_metrics, BlockArgs, TxSetupArgs, VmPermit,
    SANDBOX_METRICS,
};
use crate::execution_sandbox::vm_metrics::SandboxStage;

#[derive(Debug, Clone)]
pub(crate) struct TransactionExecutionOutput {
    /// Output of the VM.
    pub vm: VmExecutionResultAndLogs,
    /// Traced calls if requested.
    pub call_traces: Vec<Call>,
    /// Execution metrics.
    pub metrics: TransactionExecutionMetrics,
    /// Were published bytecodes OK?
    pub are_published_bytecodes_ok: bool,
}

/// Executor of transactions.
#[derive(Debug)]
pub(crate) enum TransactionExecutor {
    Real(MainOneshotExecutor),
    #[doc(hidden)] // Intended for tests only
    Mock(MockOneshotExecutor),
}

impl TransactionExecutor {
    pub fn real(missed_storage_invocation_limit: usize) -> Self {
        let mut executor = MainOneshotExecutor::new(missed_storage_invocation_limit);
        executor
            .set_execution_latency_histogram(&SANDBOX_METRICS.sandbox[&SandboxStage::Execution]);
        Self::Real(executor)
    }

    /// This method assumes that (block with number `resolved_block_number` is present in DB)
    /// or (`block_id` is `pending` and block with number `resolved_block_number - 1` is present in DB)
    #[allow(clippy::too_many_arguments)]
    #[tracing::instrument(level = "debug", skip_all)]
    pub async fn execute_tx_in_sandbox(
        &self,
        vm_permit: VmPermit,
        setup_args: TxSetupArgs,
        execution_args: TxExecutionArgs,
        connection: Connection<'static, Core>,
        block_args: BlockArgs,
        state_override: Option<StateOverride>,
        tracing_params: OneshotTracingParams,
    ) -> anyhow::Result<TransactionExecutionOutput> {
        let total_factory_deps = execution_args.transaction.execute.factory_deps.len() as u16;
        let (env, storage) =
            apply::prepare_env_and_storage(connection, setup_args, &block_args).await?;
        let state_override = state_override.unwrap_or_default();
        let storage = StorageWithOverrides::new(storage, &state_override);

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
        Ok(TransactionExecutionOutput {
            vm: *result.tx_result,
            call_traces: result.call_traces,
            metrics,
            are_published_bytecodes_ok: result.compression_result.is_ok(),
        })
    }
}

impl From<MockOneshotExecutor> for TransactionExecutor {
    fn from(executor: MockOneshotExecutor) -> Self {
        Self::Mock(executor)
    }
}

#[async_trait]
impl<S> OneshotExecutor<S> for TransactionExecutor
where
    S: ReadStorage + Send + 'static,
{
    async fn inspect_transaction_with_bytecode_compression(
        &self,
        storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        tracing_params: OneshotTracingParams,
    ) -> anyhow::Result<OneshotTransactionExecutionResult> {
        match self {
            Self::Real(executor) => {
                executor
                    .inspect_transaction_with_bytecode_compression(
                        storage,
                        env,
                        args,
                        tracing_params,
                    )
                    .await
            }
            Self::Mock(executor) => {
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
impl<S> TransactionValidator<S> for TransactionExecutor
where
    S: ReadStorage + Send + 'static,
{
    async fn validate_transaction(
        &self,
        storage: S,
        env: OneshotEnv,
        tx: L2Tx,
        validation_params: ValidationParams,
    ) -> anyhow::Result<Result<(), ValidationError>> {
        match self {
            Self::Real(executor) => {
                executor
                    .validate_transaction(storage, env, tx, validation_params)
                    .await
            }
            Self::Mock(executor) => {
                executor
                    .validate_transaction(storage, env, tx, validation_params)
                    .await
            }
        }
    }
}
