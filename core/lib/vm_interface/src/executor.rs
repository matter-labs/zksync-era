//! High-level executor traits.

use std::{fmt, marker::PhantomData};

use async_trait::async_trait;
use zksync_types::Transaction;

use crate::{
    storage::StorageView, BatchTransactionExecutionResult, FinishedL1Batch, L1BatchEnv, L2BlockEnv,
    SystemEnv,
};

/// [`BatchExecutorHandle`] outputs.
pub trait BatchExecutorOutputs {
    /// Output from processing a single transaction in a batch.
    type Tx: 'static + Send;
    /// Output from finalizing a batch.
    type Batch: 'static + Send;
}

/// Marker type for "standard" batch executor outputs.
#[derive(Debug)]
pub struct Standard<S>(PhantomData<S>);

impl<S: Send + 'static> BatchExecutorOutputs for Standard<S> {
    type Tx = BatchTransactionExecutionResult;
    type Batch = (FinishedL1Batch, StorageView<S>);
}

pub trait BatchExecutor<S: Send + 'static>: 'static + Send + fmt::Debug {
    type Outputs: BatchExecutorOutputs;
    type Handle: BatchExecutorHandle<Self::Outputs> + ?Sized;

    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> Box<Self::Handle>;
}

impl<S, T> BatchExecutor<S> for Box<T>
where
    S: Send + 'static,
    T: BatchExecutor<S> + ?Sized,
{
    type Outputs = T::Outputs;
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
pub trait BatchExecutorHandle<Out: BatchExecutorOutputs>: 'static + Send + fmt::Debug {
    async fn execute_tx(&mut self, tx: Transaction) -> anyhow::Result<Out::Tx>;

    async fn rollback_last_tx(&mut self) -> anyhow::Result<()>;

    async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()>;

    /// Returns finished batch info and a handle allowing to inspect the VM storage after the batch is finished.
    /// Note that it's always possible to return `self` as this storage handle.
    async fn finish_batch(self: Box<Self>) -> anyhow::Result<Out::Batch>;
}

/// Boxed [`BatchExecutor`]. Can be constructed from any executor using [`box_batch_executor()`].
pub type BoxBatchExecutor<S, O = Standard<S>> =
    Box<dyn BatchExecutor<S, Outputs = O, Handle = dyn BatchExecutorHandle<O>>>;

pub type DynBatchExecutorHandle<S> = dyn BatchExecutorHandle<Standard<S>>;

struct Erased<T>(T);

impl<T: fmt::Debug> fmt::Debug for Erased<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(&self.0, formatter)
    }
}

impl<S, T> BatchExecutor<S> for Erased<T>
where
    S: Send + 'static,
    T: BatchExecutor<S, Handle: Sized>,
{
    type Outputs = T::Outputs;
    type Handle = dyn BatchExecutorHandle<T::Outputs>;

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
pub fn box_batch_executor<S, T>(executor: T) -> BoxBatchExecutor<S, T::Outputs>
where
    S: Send + 'static,
    T: BatchExecutor<S, Handle: Sized>,
{
    Box::new(Erased(executor))
}
