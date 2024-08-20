//! High-level executor traits.

use async_trait::async_trait;
use zksync_types::Transaction;

use crate::{
    storage::{ReadStorage, StorageViewCache},
    BatchTransactionExecutionResult, FinishedL1Batch, L1BatchEnv, L2BlockEnv, SystemEnv,
};

pub trait BatchExecutor<S: ReadStorage> {
    type Handle: BatchExecutorHandle;

    fn init_batch(
        &mut self,
        storage: S,
        l1_batch_params: L1BatchEnv,
        system_env: SystemEnv,
    ) -> Self::Handle;
}

/// Handle for executing a single L1 batch.
#[async_trait]
pub trait BatchExecutorHandle {
    async fn execute_tx(
        &mut self,
        tx: Transaction,
    ) -> anyhow::Result<BatchTransactionExecutionResult>;

    async fn rollback_last_tx(&mut self) -> anyhow::Result<()>;

    async fn start_next_l2_block(&mut self, env: L2BlockEnv) -> anyhow::Result<()>;

    async fn finish_batch(self: Box<Self>) -> anyhow::Result<(FinishedL1Batch, StorageViewCache)>;
}
