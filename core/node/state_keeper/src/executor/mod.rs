use std::{fmt, marker::PhantomData, sync::Arc};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_multivm::interface::{
    executor::{BatchExecutor, BatchExecutorFactory, BatchExecutorOutputs, StandardOutputs},
    BatchTransactionExecutionResult, Call, CompressedBytecodeInfo, ExecutionResult,
    FinishedL1Batch, Halt, L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionResultAndLogs,
};
use zksync_state::ReadStorageFactory;
use zksync_types::Transaction;
pub use zksync_vm_executor::batch::MainBatchExecutorFactory;

use crate::ExecutionMetricsForCriteria;

#[cfg(test)]
mod tests;

/// Internal representation of a transaction executed in the virtual machine. Allows to be more typesafe
/// when dealing with halted transactions, and to test seal criteria.
#[derive(Debug, Clone)]
pub enum TxExecutionResult {
    /// Successful execution of the tx and the block tip dry run.
    Success {
        tx_result: Box<VmExecutionResultAndLogs>,
        tx_metrics: Box<ExecutionMetricsForCriteria>,
        compressed_bytecodes: Vec<CompressedBytecodeInfo>,
        call_tracer_result: Vec<Call>,
        gas_remaining: u32,
    },
    /// The VM rejected the tx for some reason.
    RejectedByVm { reason: Halt },
    /// Bootloader gas limit is not enough to execute the tx.
    BootloaderOutOfGasForTx,
}

impl TxExecutionResult {
    fn new(res: BatchTransactionExecutionResult, tx: &Transaction) -> Self {
        match res.tx_result.result {
            ExecutionResult::Halt {
                reason: Halt::BootloaderOutOfGas,
            } => Self::BootloaderOutOfGasForTx,
            ExecutionResult::Halt { reason } => Self::RejectedByVm { reason },
            _ => Self::Success {
                tx_metrics: Box::new(ExecutionMetricsForCriteria::new(Some(tx), &res.tx_result)),
                tx_result: res.tx_result,
                compressed_bytecodes: res.compressed_bytecodes,
                call_tracer_result: res.call_traces,
                gas_remaining: res.gas_remaining,
            },
        }
    }

    /// Returns a revert reason if either transaction was rejected or bootloader ran out of gas.
    pub(super) fn err(&self) -> Option<&Halt> {
        match self {
            Self::Success { .. } => None,
            Self::RejectedByVm {
                reason: rejection_reason,
            } => Some(rejection_reason),
            Self::BootloaderOutOfGasForTx => Some(&Halt::BootloaderOutOfGas),
        }
    }
}

/// Batch executor outputs consumed by the state keeper.
#[derive(Debug)]
pub struct StateKeeperOutputs(());

impl BatchExecutorOutputs for StateKeeperOutputs {
    type Tx = TxExecutionResult;
    type Batch = FinishedL1Batch;
}

pub type StateKeeperExecutor = dyn BatchExecutor<StateKeeperOutputs>;

/// Functionality of [`BatchExecutorFactory`] + [`ReadStorageFactory`] with an erased storage type. This allows to keep
/// [`ZkSyncStateKeeper`] not parameterized by the storage type, simplifying its dependency injection and usage in tests.
#[async_trait]
pub trait StateKeeperExecutorFactory: fmt::Debug + Send {
    async fn init_batch(
        &mut self,
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<Box<StateKeeperExecutor>>>;
}

/// The only [`StateKeeperExecutorFactory`] implementation.
pub struct MainStateKeeperExecutorFactory<S, T> {
    executor_factory: T,
    storage_factory: Arc<dyn ReadStorageFactory<S>>,
}

// Removes inferred `S: Debug` constraint
impl<S, T: fmt::Debug> fmt::Debug for MainStateKeeperExecutorFactory<S, T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MainStateKeeperExecutorFactory")
            .field("executor_factory", &self.executor_factory)
            .field("storage_factory", &self.storage_factory)
            .finish()
    }
}

impl<S: Send + 'static, T: BatchExecutorFactory<S>> MainStateKeeperExecutorFactory<S, T> {
    pub fn new(batch_executor: T, storage_factory: Arc<dyn ReadStorageFactory<S>>) -> Self {
        Self {
            executor_factory: batch_executor,
            storage_factory,
        }
    }
}

struct MappedExecutor<S, H: ?Sized> {
    inner: Box<H>,
    _storage: PhantomData<S>,
}

impl<S, H: ?Sized + fmt::Debug> fmt::Debug for MappedExecutor<S, H> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&*self.inner, formatter)
    }
}

#[async_trait]
impl<S, H> BatchExecutor<StateKeeperOutputs> for MappedExecutor<S, H>
where
    S: Send + 'static,
    H: BatchExecutor<StandardOutputs<S>> + ?Sized,
{
    async fn execute_tx(&mut self, tx: Transaction) -> anyhow::Result<TxExecutionResult> {
        let res = self.inner.execute_tx(tx.clone()).await?;
        Ok(TxExecutionResult::new(res, &tx))
    }

    async fn rollback_last_tx(&mut self) -> anyhow::Result<()> {
        self.inner.rollback_last_tx().await
    }

    async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()> {
        self.inner.start_next_l2_block(env).await
    }

    async fn finish_batch(self: Box<Self>) -> anyhow::Result<FinishedL1Batch> {
        let (finished_batch, _) = self.inner.finish_batch().await?;
        Ok(finished_batch)
    }
}

#[async_trait]
impl<S, T> StateKeeperExecutorFactory for MainStateKeeperExecutorFactory<S, T>
where
    S: Send + 'static,
    T: BatchExecutorFactory<S, Outputs = StandardOutputs<S>>,
{
    async fn init_batch(
        &mut self,
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<Box<StateKeeperExecutor>>> {
        let Some(storage) = self
            .storage_factory
            .access_storage(stop_receiver, l1_batch_env.number - 1)
            .await
            .context("failed creating VM storage")?
        else {
            return Ok(None);
        };
        let executor = self
            .executor_factory
            .init_batch(storage, l1_batch_env, system_env);
        Ok(Some(Box::new(MappedExecutor {
            inner: executor,
            _storage: PhantomData,
        })))
    }
}
