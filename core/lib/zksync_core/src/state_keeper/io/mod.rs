use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};

use std::{
    fmt,
    time::{Duration, Instant},
};

use multivm::interface::{FinishedL1Batch, L1BatchEnv, SystemEnv};

use zksync_dal::ConnectionPool;
use zksync_types::{
    block::MiniblockExecutionData, protocol_version::ProtocolUpgradeTx,
    witness_block_state::WitnessBlockState, L1BatchNumber, MiniblockNumber, ProtocolVersionId,
    Transaction,
};

pub(crate) mod common;
pub(crate) mod mempool;
pub(crate) mod seal_logic;

pub(crate) use self::mempool::MempoolIO;
use super::{
    metrics::{MiniblockQueueStage, MINIBLOCK_METRICS},
    seal_criteria::IoSealCriteria,
    updates::{MiniblockSealCommand, UpdatesManager},
};

#[cfg(test)]
mod tests;

/// Contains information about the un-synced execution state:
/// Batch data and transactions that were executed before and are marked as so in the DB,
/// but aren't a part of a sealed batch.
///
/// Upon a restart, we must re-execute the pending state to continue progressing from the
/// place where we stopped.
///
/// Invariant is that there may be not more than 1 pending batch, and it's always the latest batch.
#[derive(Debug)]
pub struct PendingBatchData {
    /// Data used to initialize the pending batch. We have to make sure that all the parameters
    /// (e.g. timestamp) are the same, so transaction would have the same result after re-execution.
    pub(crate) l1_batch_env: L1BatchEnv,
    pub(crate) system_env: SystemEnv,
    /// List of miniblocks and corresponding transactions that were executed within batch.
    pub(crate) pending_miniblocks: Vec<MiniblockExecutionData>,
}

#[derive(Debug, Copy, Clone, Default)]
pub struct MiniblockParams {
    /// The timestamp of the miniblock
    pub(crate) timestamp: u64,
    /// The maximal number of virtual blocks that can be created within this miniblock.
    /// During the migration from displaying users batch.number to L2 block (i.e. miniblock) number in Q3 2023
    /// in order to make the process smoother for users, we temporarily display the virtual blocks for users.
    ///
    /// Virtual blocks start their number with batch number and will increase until they reach the miniblock number.
    /// Note that it is the *maximal* number of virtual blocks that can be created within this miniblock since
    /// once the virtual blocks' number reaches the miniblock number, they will never be allowed to exceed those, i.e.
    /// any "excess" created blocks will be ignored.
    pub(crate) virtual_blocks: u32,
}

/// `StateKeeperIO` provides the interactive layer for the state keeper:
/// it's used to receive volatile parameters (such as batch parameters), and also it's used to perform
/// mutable operations on the persistent state (e.g. persist executed batches).
#[async_trait]
pub trait StateKeeperIO: 'static + Send + IoSealCriteria {
    /// Returns the number of the currently processed L1 batch.
    fn current_l1_batch_number(&self) -> L1BatchNumber;
    /// Returns the number of the currently processed miniblock (aka L2 block).
    fn current_miniblock_number(&self) -> MiniblockNumber;
    /// Returns the data on the batch that was not sealed before the server restart.
    /// See `PendingBatchData` doc-comment for details.
    async fn load_pending_batch(&mut self) -> Option<PendingBatchData>;
    /// Blocks for up to `max_wait` until the parameters for the next L1 batch are available.
    /// Returns the data required to initialize the VM for the next batch.
    async fn wait_for_new_batch_params(
        &mut self,
        max_wait: Duration,
    ) -> Option<(SystemEnv, L1BatchEnv)>;
    /// Blocks for up to `max_wait` until the parameters for the next miniblock are available.
    async fn wait_for_new_miniblock_params(
        &mut self,
        max_wait: Duration,
        prev_miniblock_timestamp: u64,
    ) -> Option<MiniblockParams>;
    /// Blocks for up to `max_wait` until the next transaction is available for execution.
    /// Returns `None` if no transaction became available until the timeout.
    async fn wait_for_next_tx(&mut self, max_wait: Duration) -> Option<Transaction>;
    /// Marks the transaction as "not executed", so it can be retrieved from the IO again.
    async fn rollback(&mut self, tx: Transaction);
    /// Marks the transaction as "rejected", e.g. one that is not correct and can't be executed.
    async fn reject(&mut self, tx: &Transaction, error: &str);
    /// Marks the miniblock (aka L2 block) as sealed.
    /// Returns the timestamp for the next miniblock.
    async fn seal_miniblock(&mut self, updates_manager: &UpdatesManager);
    /// Marks the L1 batch as sealed.
    async fn seal_l1_batch(
        &mut self,
        witness_block_state: Option<WitnessBlockState>,
        updates_manager: UpdatesManager,
        l1_batch_env: &L1BatchEnv,
        finished_batch: FinishedL1Batch,
    ) -> anyhow::Result<()>;
    /// Loads protocol version of the previous l1 batch.
    async fn load_previous_batch_version_id(&mut self) -> Option<ProtocolVersionId>;
    /// Loads protocol upgrade tx for given version.
    async fn load_upgrade_tx(&mut self, version_id: ProtocolVersionId)
        -> Option<ProtocolUpgradeTx>;
}

impl fmt::Debug for dyn StateKeeperIO {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StateKeeperIO")
            .field("current_l1_batch_number", &self.current_l1_batch_number())
            .field("current_miniblock_number", &self.current_miniblock_number())
            .finish()
    }
}

/// A command together with the return address allowing to track command processing completion.
#[derive(Debug)]
struct Completable<T> {
    command: T,
    completion_sender: oneshot::Sender<()>,
}

/// Handle for [`MiniblockSealer`] allowing to submit [`MiniblockSealCommand`]s.
#[derive(Debug)]
pub(crate) struct MiniblockSealerHandle {
    commands_sender: mpsc::Sender<Completable<MiniblockSealCommand>>,
    latest_completion_receiver: Option<oneshot::Receiver<()>>,
    // If true, `submit()` will wait for the operation to complete.
    is_sync: bool,
}

impl MiniblockSealerHandle {
    const SHUTDOWN_MSG: &'static str = "miniblock sealer unexpectedly shut down";

    /// Submits a new sealing `command` to the sealer that this handle is attached to.
    ///
    /// If there are currently too many unprocessed commands, this method will wait until
    /// enough of them are processed (i.e., there is backpressure).
    pub async fn submit(&mut self, command: MiniblockSealCommand) {
        let miniblock_number = command.miniblock_number;
        tracing::debug!(
            "Enqueuing sealing command for miniblock #{miniblock_number} with #{} txs (L1 batch #{})",
            command.miniblock.executed_transactions.len(),
            command.l1_batch_number
        );

        let start = Instant::now();
        let (completion_sender, completion_receiver) = oneshot::channel();
        self.latest_completion_receiver = Some(completion_receiver);
        let command = Completable {
            command,
            completion_sender,
        };
        self.commands_sender
            .send(command)
            .await
            .expect(Self::SHUTDOWN_MSG);

        let elapsed = start.elapsed();
        let queue_capacity = self.commands_sender.capacity();
        tracing::debug!(
            "Enqueued sealing command for miniblock #{miniblock_number} (took {elapsed:?}; \
             available queue capacity: {queue_capacity})"
        );

        if self.is_sync {
            self.wait_for_all_commands().await;
        } else {
            MINIBLOCK_METRICS.seal_queue_capacity.set(queue_capacity);
            MINIBLOCK_METRICS.seal_queue_latency[&MiniblockQueueStage::Submit].observe(elapsed);
        }
    }

    /// Waits until all previously submitted commands are fully processed by the sealer.
    pub async fn wait_for_all_commands(&mut self) {
        tracing::debug!(
            "Requested waiting for miniblock seal queue to empty; current available capacity: {}",
            self.commands_sender.capacity()
        );

        let start = Instant::now();
        let completion_receiver = self.latest_completion_receiver.take();
        if let Some(completion_receiver) = completion_receiver {
            completion_receiver.await.expect(Self::SHUTDOWN_MSG);
        }

        let elapsed = start.elapsed();
        tracing::debug!("Miniblock seal queue is emptied (took {elapsed:?})");

        // Since this method called from outside is essentially a no-op if `self.is_sync`,
        // we don't report its metrics in this case.
        if !self.is_sync {
            MINIBLOCK_METRICS
                .seal_queue_capacity
                .set(self.commands_sender.capacity());
            MINIBLOCK_METRICS.seal_queue_latency[&MiniblockQueueStage::WaitForAllCommands]
                .observe(elapsed);
        }
    }
}

/// Component responsible for sealing miniblocks (i.e., storing their data to Postgres).
#[derive(Debug)]
pub(crate) struct MiniblockSealer {
    pool: ConnectionPool,
    is_sync: bool,
    // Weak sender handle to get queue capacity stats.
    commands_sender: mpsc::WeakSender<Completable<MiniblockSealCommand>>,
    commands_receiver: mpsc::Receiver<Completable<MiniblockSealCommand>>,
}

impl MiniblockSealer {
    /// Creates a sealer that will use the provided Postgres connection and will have the specified
    /// `command_capacity` for unprocessed sealing commands.
    pub(crate) fn new(
        pool: ConnectionPool,
        mut command_capacity: usize,
    ) -> (Self, MiniblockSealerHandle) {
        let is_sync = command_capacity == 0;
        command_capacity = command_capacity.max(1);

        let (commands_sender, commands_receiver) = mpsc::channel(command_capacity);
        let this = Self {
            pool,
            is_sync,
            commands_sender: commands_sender.downgrade(),
            commands_receiver,
        };
        let handle = MiniblockSealerHandle {
            commands_sender,
            latest_completion_receiver: None,
            is_sync,
        };
        (this, handle)
    }

    /// Seals miniblocks as they are received from the [`MiniblockSealerHandle`]. This should be run
    /// on a separate Tokio task.
    pub async fn run(mut self) -> anyhow::Result<()> {
        if self.is_sync {
            tracing::info!("Starting synchronous miniblock sealer");
        } else if let Some(sender) = self.commands_sender.upgrade() {
            tracing::info!(
                "Starting async miniblock sealer with queue capacity {}",
                sender.max_capacity()
            );
        } else {
            tracing::warn!("Miniblock sealer not started, since its handle is already dropped");
        }

        let mut miniblock_seal_delta: Option<Instant> = None;
        // Commands must be processed sequentially: a later miniblock cannot be saved before
        // an earlier one.
        while let Some(completable) = self.next_command().await {
            let mut conn = self
                .pool
                .access_storage_tagged("state_keeper")
                .await
                .unwrap();
            completable.command.seal(&mut conn).await;
            if let Some(delta) = miniblock_seal_delta {
                MINIBLOCK_METRICS.seal_delta.observe(delta.elapsed());
            }
            miniblock_seal_delta = Some(Instant::now());

            completable.completion_sender.send(()).ok();
            // ^ We don't care whether anyone listens to the processing progress
        }
        Ok(())
    }

    async fn next_command(&mut self) -> Option<Completable<MiniblockSealCommand>> {
        tracing::debug!("Polling miniblock seal queue for next command");
        let start = Instant::now();
        let command = self.commands_receiver.recv().await;
        let elapsed = start.elapsed();

        if let Some(completable) = &command {
            tracing::debug!(
                "Received command to seal miniblock #{} (polling took {elapsed:?})",
                completable.command.miniblock_number
            );
        }

        if !self.is_sync {
            MINIBLOCK_METRICS.seal_queue_latency[&MiniblockQueueStage::NextCommand]
                .observe(elapsed);
            if let Some(sender) = self.commands_sender.upgrade() {
                MINIBLOCK_METRICS.seal_queue_capacity.set(sender.capacity());
            }
        }
        command
    }
}
