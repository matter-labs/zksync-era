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

/// Hook allowing to inspect / modify VM storage during or after batch execution. Used in [`InspectStorage`].
pub type InspectStorageFn<S> = Box<dyn FnOnce(&mut StorageView<S>) + Send>;

/// Allows to inspect / modify VM storage during or after batch execution.
///
/// `async` and the `Send` constraint on the hook are necessary because the storage and VM are `!Send`
/// and cannot be sent to the calling thread.
#[async_trait]
pub trait InspectStorage<S>: 'static + Send + fmt::Debug {
    /// Inspects the current storage state.
    async fn inspect_storage(&mut self, f: InspectStorageFn<S>) -> anyhow::Result<()>;
}

/// Handle for executing a single L1 batch.
///
/// The handle is parametric by the transaction execution output in order to be able to represent different
/// levels of abstraction. The default value is intended to be most generic.
#[async_trait]
pub trait BatchExecutorHandle<S, R = BatchTransactionExecutionResult>: InspectStorage<S> {
    async fn execute_tx(&mut self, tx: Transaction) -> anyhow::Result<R>;

    async fn rollback_last_tx(&mut self) -> anyhow::Result<()>;

    async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()>;

    /// Returns finished batch info and a handle allowing to inspect the VM storage after the batch is finished.
    /// Note that it's always possible to return `self` as this storage handle.
    async fn finish_batch(
        self: Box<Self>,
    ) -> anyhow::Result<(FinishedL1Batch, Box<dyn InspectStorage<S>>)>;
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
