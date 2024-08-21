//! High-level executor traits.

use std::{fmt, marker::PhantomData};

use async_trait::async_trait;
use zksync_types::Transaction;

use crate::{
    storage::StorageView, BatchTransactionExecutionResult, FinishedL1Batch, L1BatchEnv, L2BlockEnv,
    SystemEnv,
};

/// [`BatchExecutor`] outputs.
pub trait BatchExecutorOutputs {
    /// Output from processing a single transaction in a batch.
    type Tx: 'static + Send;
    /// Output from finalizing a batch.
    type Batch: 'static + Send;
}

/// Marker type for "standard" batch executor outputs.
#[derive(Debug)]
pub struct StandardOutputs<S>(PhantomData<S>);

impl<S: Send + 'static> BatchExecutorOutputs for StandardOutputs<S> {
    type Tx = BatchTransactionExecutionResult;
    type Batch = (FinishedL1Batch, StorageView<S>);
}

pub trait BatchExecutorFactory<S: Send + 'static>: 'static + Send + fmt::Debug {
    type Outputs: BatchExecutorOutputs;
    type Executor: BatchExecutor<Self::Outputs> + ?Sized;

    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> Box<Self::Executor>;
}

impl<S, T> BatchExecutorFactory<S> for Box<T>
where
    S: Send + 'static,
    T: BatchExecutorFactory<S> + ?Sized,
{
    type Outputs = T::Outputs;
    type Executor = T::Executor;

    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> Box<Self::Executor> {
        (**self).init_batch(storage, l1_batch_params, system_env)
    }
}

/// Handle for executing a single L1 batch.
///
/// The handle is parametric by the transaction execution output in order to be able to represent different
/// levels of abstraction.
#[async_trait]
pub trait BatchExecutor<Out: BatchExecutorOutputs>: 'static + Send + fmt::Debug {
    /// Executes a transaction.
    async fn execute_tx(&mut self, tx: Transaction) -> anyhow::Result<Out::Tx>;

    /// Rolls back the last executed transaction.
    async fn rollback_last_tx(&mut self) -> anyhow::Result<()>;

    /// Starts a next L2 block with the specified params.
    async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()>;

    /// Finished the current L1 batch.
    async fn finish_batch(self: Box<Self>) -> anyhow::Result<Out::Batch>;
}

/// Boxed [`BatchExecutorFactory`]. Can be constructed from any executor using [`box_batch_executor_factory()`].
pub type BoxBatchExecutorFactory<S, O = StandardOutputs<S>> =
    Box<dyn BatchExecutorFactory<S, Outputs = O, Executor = dyn BatchExecutor<O>>>;

/// Trait object for [`BatchExecutor`] with [`StandardOutputs`].
pub type DynBatchExecutor<S> = dyn BatchExecutor<StandardOutputs<S>>;

/// Wrapper for a [`BatchExecutorFactory`] erasing returned executors.
struct ErasedFactory<T>(T);

impl<T: fmt::Debug> fmt::Debug for ErasedFactory<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

impl<S, T> BatchExecutorFactory<S> for ErasedFactory<T>
where
    S: Send + 'static,
    T: BatchExecutorFactory<S, Executor: Sized>,
{
    type Outputs = T::Outputs;
    type Executor = dyn BatchExecutor<T::Outputs>;

    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> Box<Self::Executor> {
        self.0.init_batch(storage, l1_batch_params, system_env)
    }
}

/// Boxes the provided executor factory so that it doesn't have an ambiguous associated type.
pub fn box_batch_executor_factory<S, T>(executor: T) -> BoxBatchExecutorFactory<S, T::Outputs>
where
    S: Send + 'static,
    T: BatchExecutorFactory<S, Executor: Sized>,
{
    Box::new(ErasedFactory(executor))
}
