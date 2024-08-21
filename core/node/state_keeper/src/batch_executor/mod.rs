use std::{fmt, sync::Arc};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_multivm::interface::{
    executor::{BatchExecutor, BatchExecutorHandle, BatchExecutorOutputs, Standard},
    BatchTransactionExecutionResult, Call, CompressedBytecodeInfo, ExecutionResult,
    FinishedL1Batch, Halt, L1BatchEnv, L2BlockEnv, SystemEnv, VmExecutionResultAndLogs,
};
use zksync_state::{OwnedStorage, ReadStorageFactory};
use zksync_types::Transaction;
use zksync_vm_utils::batch::MainBatchExecutor;

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

pub type StateKeeperExecutorHandle = dyn BatchExecutorHandle<StateKeeperOutputs>;

// FIXME: remove by using `BatchExecutor<()>`?
/// Functionality of [`BatchExecutor`] + [`ReadStorageFactory`] with an erased storage type. This allows to keep
/// [`ZkSyncStateKeeper`] not parameterized by the storage type, simplifying its dependency injection and usage in tests.
#[async_trait]
pub trait StateKeeperExecutor: fmt::Debug + Send {
    async fn init_batch(
        &mut self,
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<Box<StateKeeperExecutorHandle>>>;
}

/// The only [`crate::keeper::ErasedBatchExecutor`] implementation.
#[derive(Debug)]
pub struct MainStateKeeperExecutor<E> {
    batch_executor: E,
    storage_factory: Arc<dyn ReadStorageFactory<OwnedStorage>>,
}

impl MainStateKeeperExecutor<MainBatchExecutor> {
    pub fn new(
        save_call_traces: bool,
        optional_bytecode_compression: bool,
        storage_factory: Arc<dyn ReadStorageFactory<OwnedStorage>>,
    ) -> Self {
        Self {
            batch_executor: MainBatchExecutor::new(save_call_traces, optional_bytecode_compression),
            storage_factory,
        }
    }
}

impl<E: BatchExecutor<OwnedStorage>> MainStateKeeperExecutor<E> {
    pub fn custom(
        batch_executor: E,
        storage_factory: Arc<dyn ReadStorageFactory<OwnedStorage>>,
    ) -> Self {
        Self {
            batch_executor,
            storage_factory,
        }
    }
}

#[derive(Debug)]
struct MappedHandle<H: ?Sized>(Box<H>);

#[async_trait]
impl<H> BatchExecutorHandle<StateKeeperOutputs> for MappedHandle<H>
where
    H: BatchExecutorHandle<Standard<OwnedStorage>> + ?Sized,
{
    async fn execute_tx(&mut self, tx: Transaction) -> anyhow::Result<TxExecutionResult> {
        let res = self.0.execute_tx(tx.clone()).await?;
        Ok(TxExecutionResult::new(res, &tx))
    }

    async fn rollback_last_tx(&mut self) -> anyhow::Result<()> {
        self.0.rollback_last_tx().await
    }

    async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()> {
        self.0.start_next_l2_block(env).await
    }

    async fn finish_batch(self: Box<Self>) -> anyhow::Result<FinishedL1Batch> {
        let (finished_batch, _) = self.0.finish_batch().await?;
        // FIXME: metrics
        Ok(finished_batch)
    }
}

#[async_trait]
impl<E> StateKeeperExecutor for MainStateKeeperExecutor<E>
where
    E: BatchExecutor<OwnedStorage, Outputs = Standard<OwnedStorage>>,
{
    async fn init_batch(
        &mut self,
        l1_batch_env: L1BatchEnv,
        system_env: SystemEnv,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<Box<StateKeeperExecutorHandle>>> {
        let Some(storage) = self
            .storage_factory
            .access_storage(stop_receiver, l1_batch_env.number - 1)
            .await
            .context("failed creating VM storage")?
        else {
            return Ok(None);
        };
        let handle = self
            .batch_executor
            .init_batch(storage, l1_batch_env, system_env);
        Ok(Some(Box::new(MappedHandle(handle))))
    }
}
