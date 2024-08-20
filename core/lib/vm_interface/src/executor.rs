//! High-level executor traits.

use std::fmt;

use async_trait::async_trait;
use zksync_types::Transaction;

use crate::{
    storage::StorageView, BatchTransactionExecutionResult, FinishedL1Batch, L1BatchEnv, L2BlockEnv,
    SystemEnv,
};

pub trait BatchExecutor<S>: 'static + Send + fmt::Debug {
    type Handle: BatchExecutorHandle<S> + ?Sized;

    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> Box<Self::Handle>;
}

/// Hook allowing to inspect / modify VM storage after finishing the batch.
/// Necessary because the storage is `!Send` and cannot be sent to the calling thread.
pub type InspectStorageFn<S> = Box<dyn FnOnce(&mut StorageView<S>) + Send>;

impl<S, T: BatchExecutor<S> + ?Sized> BatchExecutor<S> for Box<T> {
    type Handle = T::Handle;

    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> Box<Self::Handle> {
        (**self).init_batch(storage, l1_batch_params, system_env)
    }
}

/// Handle for executing a single L1 batch.
///
/// The handle is parametric by the transaction execution output in order to be able to represent different
/// levels of abstraction. The default value is intended to be most generic.
#[async_trait]
pub trait BatchExecutorHandle<S, R = BatchTransactionExecutionResult>:
    'static + Send + fmt::Debug
{
    async fn execute_tx(&mut self, tx: Transaction) -> anyhow::Result<R>;

    async fn rollback_last_tx(&mut self) -> anyhow::Result<()>;

    async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()>;

    /// Logically, this is the last operation, but it doesn't consume `self` to allow inspecting storage afterward.
    async fn finish_batch(&mut self) -> anyhow::Result<FinishedL1Batch>;

    /// Inspects the current storage state.
    async fn inspect_storage(&mut self, f: InspectStorageFn<S>) -> anyhow::Result<()>;
}

/// Boxed [`BatchExecutor`]. Can be constructed from any executor using [`box_batch_executor()`].
pub type BoxBatchExecutor<S> = Box<dyn BatchExecutor<S, Handle = dyn BatchExecutorHandle<S>>>;

struct Erased<T>(T);

impl<T: fmt::Debug> fmt::Debug for Erased<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

impl<S: 'static, T: BatchExecutor<S, Handle: Sized>> BatchExecutor<S> for Erased<T> {
    type Handle = dyn BatchExecutorHandle<S>;

    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> Box<Self::Handle> {
        self.0.init_batch(storage, l1_batch_params, system_env)
    }
}

/// Boxes the provided executor so that it doesn't have an ambiguous associated type.
pub fn box_batch_executor<S: 'static>(
    executor: impl BatchExecutor<S, Handle: Sized>,
) -> BoxBatchExecutor<S> {
    Box::new(Erased(executor))
}
