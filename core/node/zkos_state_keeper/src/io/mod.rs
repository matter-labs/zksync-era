use std::{fmt, time::Duration};

use anyhow::Context;
use async_trait::async_trait;
pub use output_handler::{OutputHandler, StateKeeperOutputHandler};
pub use persistence::{BlockPersistenceTask, StateKeeperPersistence};
use zksync_dal::{Connection, Core, CoreDal};
use zksync_state_keeper::seal_criteria::UnexecutableReason;
use zksync_types::{
    block::UnsealedL1BatchHeader, fee_model::BatchFeeInput, L1BatchNumber, L2BlockNumber,
    L2ChainId, ProtocolVersionId, Transaction,
};

use crate::seal_criteria::IoSealCriterion;

pub mod mempool;
mod output_handler;
mod persistence;
pub mod seal_logic;

/// Cursor of the L2 block / L1 batch progress used by `StateKeeperIO` implementations.
#[derive(Debug)]
pub struct IoCursor {
    pub next_l2_block: L2BlockNumber,
    pub l1_batch: L1BatchNumber,
}

impl IoCursor {
    /// Loads the cursor from Postgres.
    pub async fn new(storage: &mut Connection<'_, Core>) -> anyhow::Result<Self> {
        let last_sealed_l1_batch_number = storage.blocks_dal().get_sealed_l1_batch_number().await?;
        let last_sealed_l2_block_number = storage.blocks_dal().get_sealed_l2_block_number().await?;

        if let (Some(l1_batch_number), Some(l2_block_number)) =
            (last_sealed_l1_batch_number, last_sealed_l2_block_number)
        {
            Ok(Self {
                next_l2_block: l2_block_number + 1,
                l1_batch: l1_batch_number + 1,
            })
        } else {
            let snapshot_recovery = storage
                .snapshot_recovery_dal()
                .get_applied_snapshot_status()
                .await?
                .context("Postgres contains neither blocks nor snapshot recovery info")?;
            let l1_batch =
                last_sealed_l1_batch_number.unwrap_or(snapshot_recovery.l1_batch_number) + 1;

            let next_l2_block = if let Some(l2_block_number) = last_sealed_l2_block_number {
                l2_block_number + 1
            } else {
                snapshot_recovery.l2_block_number + 1
            };

            Ok(Self {
                next_l2_block,
                l1_batch,
            })
        }
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq)]
pub struct BlockParams {
    /// The timestamp of the L2 block is ms.
    pub timestamp_ms: u128,
    /// Fee parameters to be used in the new L1 batch.
    pub fee_input: BatchFeeInput,
    /// Protocol version for the new L1 batch.
    pub protocol_version: ProtocolVersionId,
    // TODO: should be derivable from fee input?
    pub base_fee: u64,
}

impl BlockParams {
    pub fn timestamp(&self) -> u64 {
        u64::try_from(self.timestamp_ms).unwrap()
    }
}

/// Provides the interactive layer for the state keeper:
/// it's used to receive volatile parameters (such as block parameters) and sequence transactions
/// providing L2 block and L1 batch boundaries for them.
///
/// Methods with `&self` receiver must be cancel-safe; i.e., they should not use interior mutability
/// to change the I/O state. Methods with `&mut self` receiver don't need to be cancel-safe.
///
/// All errors returned from this method are treated as unrecoverable.
#[async_trait]
pub trait StateKeeperIO: 'static + Send + Sync + fmt::Debug + IoSealCriterion {
    /// Returns the ID of the L2 chain. This ID is supposed to be static.
    fn chain_id(&self) -> L2ChainId;

    /// Returns the data on the batch that was not sealed before the server restart.
    /// See `PendingBatchData` doc-comment for details.
    async fn initialize(&mut self) -> anyhow::Result<IoCursor>;

    /// Blocks for up to `max_wait` until the parameters for the next L2 block are available.
    async fn wait_for_new_l2_block_params(
        &mut self,
        cursor: &IoCursor,
        max_wait: Duration,
    ) -> anyhow::Result<(Option<BlockParams>, Option<UnsealedL1BatchHeader>)>;

    /// Blocks for up to `max_wait` until the next transaction is available for execution.
    /// Returns `None` if no transaction became available until the timeout.
    async fn wait_for_next_tx(
        &mut self,
        max_wait: Duration,
        l2_block_timestamp: u64,
    ) -> anyhow::Result<Option<Transaction>>;

    /// Marks the transaction as "not executed", so it can be retrieved from the IO again.
    async fn rollback(&mut self, tx: Transaction) -> anyhow::Result<()>;

    /// Marks the transaction as "rejected", e.g. one that is not correct and can't be executed.
    async fn reject(&mut self, tx: &Transaction, reason: UnexecutableReason) -> anyhow::Result<()>;
}
