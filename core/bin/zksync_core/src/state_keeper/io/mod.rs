use std::time::Duration;

use vm::vm_with_bootloader::BlockContextMode;
use vm::vm_with_bootloader::DerivedBlockContext;
use vm::zk_evm::block_properties::BlockProperties;
use vm::VmBlockResult;
use zksync_types::{L1BatchNumber, MiniblockNumber, Transaction};

use crate::state_keeper::updates::UpdatesManager;

pub(crate) use mempool::MempoolIO;

mod mempool;

/// Contains information about the un-synced execution state:
/// Batch data and transactions that were executed before and are marked as so in the DB,
/// but aren't a part of a sealed batch.
///
/// Upon a restart, we must re-execute the pending state to continue progressing from the
/// place where we stopped.
///
/// Invariant is that there may be not more than 1 pending batch, and it's always the latest batch.
#[derive(Debug)]
pub(crate) struct PendingBatchData {
    /// Data used to initialize the pending batch. We have to make sure that all the parameters
    /// (e.g. timestamp) are the same, so transaction would have the same result after re-execution.
    pub(crate) params: (BlockContextMode, BlockProperties),
    /// List of miniblocks and corresponding transactions that were executed within batch.
    pub(crate) txs: Vec<(MiniblockNumber, Vec<Transaction>)>,
}

/// `StateKeeperIO` provides the interactive layer for the state keeper:
/// it's used to receive volatile parameters (such as batch parameters), and also it's used to perform
/// mutable operations on the persistent state (e.g. persist executed batches).
pub(crate) trait StateKeeperIO: 'static + std::fmt::Debug + Send {
    /// Returns the number of the currently processed L1 batch.
    fn current_l1_batch_number(&self) -> L1BatchNumber;
    /// Returns the number of the currently processed miniblock (aka L2 block).
    fn current_miniblock_number(&self) -> MiniblockNumber;
    /// Returns the data on the batch that was not sealed before the server restart.
    /// See `PendingBatchData` doc-comment for details.
    fn load_pending_batch(&mut self) -> Option<PendingBatchData>;
    /// Blocks for up to `max_wait` until the parameters for the next L1 batch are available.
    /// Returns the data required to initialize the VM for the next batch.
    fn wait_for_new_batch_params(
        &mut self,
        max_wait: Duration,
    ) -> Option<(BlockContextMode, BlockProperties)>;
    /// Blocks for up to `max_wait` until the next transaction is available for execution.
    /// Returns `None` if no transaction became available until the timeout.
    fn wait_for_next_tx(&mut self, max_wait: Duration) -> Option<Transaction>;
    /// Marks the transaction as "not executed", so it can be retrieved from the IO again.
    fn rollback(&mut self, tx: &Transaction);
    /// Marks the transaction as "rejected", e.g. one that is not correct and can't be executed.
    fn reject(&mut self, tx: &Transaction, error: &str);
    /// Marks the miniblock (aka L2 block) as sealed.
    /// Returns the timestamp for the next miniblock.
    fn seal_miniblock(&mut self, updates_manager: &UpdatesManager) -> u64;
    /// Marks the L1 batch as sealed.
    fn seal_l1_batch(
        &mut self,
        block_result: VmBlockResult,
        updates_manager: UpdatesManager,
        block_context: DerivedBlockContext,
    );
}
