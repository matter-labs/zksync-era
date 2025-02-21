use std::{fmt, time::Duration};

use async_trait::async_trait;
pub use output_handler::{OutputHandler, StateKeeperOutputHandler};
pub use persistence::StateKeeperPersistence;
use zksync_state_keeper::{io::IoCursor, seal_criteria::UnexecutableReason};
use zksync_types::{fee_model::BatchFeeInput, L2ChainId, ProtocolVersionId, Transaction};

pub mod mempool;
mod output_handler;
mod persistence;
pub mod seal_logic;

#[derive(Debug, Copy, Clone, Default, PartialEq)]
pub struct BlockParams {
    /// The timestamp of the L2 block.
    pub timestamp: u64,
    /// Fee parameters to be used in the new L1 batch.
    pub fee_input: BatchFeeInput,
    pub protocol_version: ProtocolVersionId,
    pub base_fee: u64,
}

/// Provides the interactive layer for the state keeper:
/// it's used to receive volatile parameters (such as batch parameters) and sequence transactions
/// providing L2 block and L1 batch boundaries for them.
///
/// Methods with `&self` receiver must be cancel-safe; i.e., they should not use interior mutability
/// to change the I/O state. Methods with `&mut self` receiver don't need to be cancel-safe.
///
/// All errors returned from this method are treated as unrecoverable.
#[async_trait]
pub trait StateKeeperIO: 'static + Send + Sync + fmt::Debug {
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
    ) -> anyhow::Result<Option<BlockParams>>;

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
