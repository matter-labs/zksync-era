//! High-level executor traits.

use std::fmt;

use async_trait::async_trait;
use zksync_types::Transaction;

use crate::{
    storage::StorageView, BatchTransactionExecutionResult, FinishedL1Batch, L1BatchEnv, L2BlockEnv,
    SystemEnv,
};

/// Factory of [`BatchExecutor`]s.
pub trait BatchExecutorFactory<S: Send + 'static>: 'static + Send + fmt::Debug {
    /// Initializes an executor for a batch with the specified params and using the provided storage.
    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> Box<dyn BatchExecutor<S>>;
}

/// Handle for executing a single L1 batch.
///
/// The handle is parametric by the transaction execution output in order to be able to represent different
/// levels of abstraction.
#[async_trait]
pub trait BatchExecutor<S>: 'static + Send + fmt::Debug {
    /// Executes a transaction.
    async fn execute_tx(
        &mut self,
        tx: Transaction,
    ) -> anyhow::Result<BatchTransactionExecutionResult>;

    /// Rolls back the last executed transaction.
    async fn rollback_last_tx(&mut self) -> anyhow::Result<()>;

    /// Starts a next L2 block with the specified params.
    async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()>;

    /// Finished the current L1 batch.
    async fn finish_batch(self: Box<Self>) -> anyhow::Result<(FinishedL1Batch, StorageView<S>)>;
}
