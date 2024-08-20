//! High-level executor traits.

use std::fmt;

use async_trait::async_trait;
use zksync_types::Transaction;

use crate::{
    storage::StorageViewCache, BatchTransactionExecutionResult, FinishedL1Batch, L1BatchEnv,
    L2BlockEnv, SystemEnv,
};

pub trait BatchExecutor<S>: 'static + Send + fmt::Debug {
    type Handle: BatchExecutorHandle + ?Sized;

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

/// Handle for executing a single L1 batch.
///
/// The handle is parametric by the transaction execution output in order to be able to represent different
/// levels of abstraction. The default value is intended to be most generic.
#[async_trait]
pub trait BatchExecutorHandle<R = BatchTransactionExecutionResult>:
    'static + Send + fmt::Debug
{
    async fn execute_tx(&mut self, tx: Transaction) -> anyhow::Result<R>;

    async fn rollback_last_tx(&mut self) -> anyhow::Result<()>;

    async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()>;

    // FIXME: remove cache
    async fn finish_batch(self: Box<Self>) -> anyhow::Result<(FinishedL1Batch, StorageViewCache)>;
}

pub type BoxBatchExecutor<S> = Box<dyn BatchExecutor<S, Handle = dyn BatchExecutorHandle>>;

struct Erased<T>(T);

impl<T: fmt::Debug> fmt::Debug for Erased<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

impl<S, T: BatchExecutor<S, Handle: Sized>> BatchExecutor<S> for Erased<T> {
    type Handle = dyn BatchExecutorHandle;

    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> Box<Self::Handle> {
        self.0.init_batch(storage, l1_batch_params, system_env)
    }
}

pub fn box_batch_executor<S>(
    executor: impl BatchExecutor<S, Handle: Sized>,
) -> BoxBatchExecutor<S> {
    Box::new(Erased(executor))
}
