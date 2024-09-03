//! Implementation of "executing" methods, e.g. `eth_call`.

use async_trait::async_trait;
use zksync_dal::{Connection, Core};
use zksync_multivm::interface::{
    executor::OneshotExecutor, storage::ReadStorage, BytecodeCompressionError, OneshotEnv,
    OneshotTracers, TransactionExecutionMetrics, TxExecutionArgs, VmExecutionResultAndLogs,
};
use zksync_types::api::state_override::StateOverride;
use zksync_vm_executor::oneshot::{MainOneshotExecutor, MockOneshotExecutor};

use super::{apply, storage::StorageWithOverrides, vm_metrics, BlockArgs, TxSetupArgs, VmPermit};

#[derive(Debug, Clone)]
pub(crate) struct TransactionExecutionOutput {
    /// Output of the VM.
    pub vm: VmExecutionResultAndLogs,
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
        Self::Real(MainOneshotExecutor::new(missed_storage_invocation_limit))
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
        tracers: OneshotTracers<'_>,
    ) -> anyhow::Result<TransactionExecutionOutput> {
        let total_factory_deps = execution_args.transaction.execute.factory_deps.len() as u16;
        let (env, storage) =
            apply::prepare_env_and_storage(connection, setup_args, &block_args).await?;
        let state_override = state_override.unwrap_or_default();
        let storage = StorageWithOverrides::new(storage, &state_override);

        let (published_bytecodes, execution_result) = self
            .inspect_transaction_with_bytecode_compression(storage, env, execution_args, tracers)
            .await?;
        drop(vm_permit);

        let metrics =
            vm_metrics::collect_tx_execution_metrics(total_factory_deps, &execution_result);
        Ok(TransactionExecutionOutput {
            vm: execution_result,
            metrics,
            are_published_bytecodes_ok: published_bytecodes.is_ok(),
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
    async fn inspect_transaction(
        &self,
        storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        tracers: OneshotTracers<'_>,
    ) -> anyhow::Result<VmExecutionResultAndLogs> {
        match self {
            Self::Real(executor) => {
                executor
                    .inspect_transaction(storage, env, args, tracers)
                    .await
            }
            Self::Mock(executor) => {
                executor
                    .inspect_transaction(storage, env, args, tracers)
                    .await
            }
        }
    }

    async fn inspect_transaction_with_bytecode_compression(
        &self,
        storage: S,
        env: OneshotEnv,
        args: TxExecutionArgs,
        tracers: OneshotTracers<'_>,
    ) -> anyhow::Result<(
        Result<(), BytecodeCompressionError>,
        VmExecutionResultAndLogs,
    )> {
        match self {
            Self::Real(executor) => {
                executor
                    .inspect_transaction_with_bytecode_compression(storage, env, args, tracers)
                    .await
            }
            Self::Mock(executor) => {
                executor
                    .inspect_transaction_with_bytecode_compression(storage, env, args, tracers)
                    .await
            }
        }
    }
}
