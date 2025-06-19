//! State keeper persistence logic.

use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_types::block::UnsealedL1BatchHeader;

use crate::{
    io::{
        seal_logic::l2_block_seal_subtasks::L2BlockSealProcess, IoCursor, StateKeeperOutputHandler,
    },
    updates::{BlockSealCommand, FinishedBlock},
};

/// Canonical [`StateKeeperOutputHandler`] implementation that queues storing of L2 blocks and L1 batches to Postgres.
#[derive(Debug)]
pub struct StateKeeperPersistence {
    pool: ConnectionPool<Core>,
    pre_insert_txs: bool,
    commands_sender: mpsc::Sender<Completable<PersistenceCommand>>,
    latest_completion_receiver: Option<oneshot::Receiver<()>>,
    // If true, submitting new command will wait for the operation to complete.
    is_sync: bool,
}

impl StateKeeperPersistence {
    const SHUTDOWN_MSG: &'static str = "L2 block sealer unexpectedly shut down";

    pub async fn new(
        pool: ConnectionPool<Core>,
        mut command_capacity: usize,
    ) -> anyhow::Result<(Self, BlockPersistenceTask)> {
        let is_sync = command_capacity == 0;
        command_capacity = command_capacity.max(1);

        let (commands_sender, commands_receiver) = mpsc::channel(command_capacity);
        let sealer = BlockPersistenceTask {
            pool: pool.clone(),
            is_sync,
            commands_sender: commands_sender.downgrade(),
            commands_receiver,
        };

        let this = Self {
            pool,
            pre_insert_txs: false,
            commands_sender,
            latest_completion_receiver: None,
            is_sync,
        };
        Ok((this, sealer))
    }

    pub fn with_tx_insertion(mut self) -> Self {
        self.pre_insert_txs = true;
        self
    }

    async fn submit_block(&mut self, command: BlockSealCommand) {
        let l1_batch_number = command.inner.l1_batch_number;
        tracing::info!("Enqueuing sealing command for L1 batch #{l1_batch_number}");

        let start = Instant::now();
        let (completion_sender, completion_receiver) = oneshot::channel();
        self.latest_completion_receiver = Some(completion_receiver);
        let command = Completable {
            command: PersistenceCommand::BlockSeal(command),
            completion_sender,
        };
        self.commands_sender
            .send(command)
            .await
            .expect(Self::SHUTDOWN_MSG);

        let elapsed = start.elapsed();
        let queue_capacity = self.commands_sender.capacity();
        tracing::info!(
            "Enqueued sealing command for L1 batch #{l1_batch_number} (took {elapsed:?}; \
             available queue capacity: {queue_capacity})"
        );

        if self.is_sync {
            self.wait_for_all_commands().await;
        } else {
            // L2_BLOCK_METRICS.seal_queue_capacity.set(queue_capacity);
            // L2_BLOCK_METRICS.seal_queue_latency[&L2BlockQueueStage::Submit].observe(elapsed);
        }
    }

    async fn submit_opened_batch(&mut self, header: UnsealedL1BatchHeader) {
        let l1_batch_number = header.number;
        tracing::info!("Enqueuing open batch command for L1 batch #{l1_batch_number}");

        let start = Instant::now();
        let (completion_sender, completion_receiver) = oneshot::channel();
        self.latest_completion_receiver = Some(completion_receiver);
        let command = Completable {
            command: PersistenceCommand::OpenBatch(header),
            completion_sender,
        };
        self.commands_sender
            .send(command)
            .await
            .expect(Self::SHUTDOWN_MSG);

        let elapsed = start.elapsed();
        let queue_capacity = self.commands_sender.capacity();
        tracing::info!(
            "Enqueued open batch command for L1 batch #{l1_batch_number} (took {elapsed:?}; \
             available queue capacity: {queue_capacity})"
        );

        if self.is_sync {
            self.wait_for_all_commands().await;
        } else {
            // L2_BLOCK_METRICS.seal_queue_capacity.set(queue_capacity);
            // L2_BLOCK_METRICS.seal_queue_latency[&L2BlockQueueStage::Submit].observe(elapsed);
        }
    }

    /// Waits until all previously submitted commands are fully processed by the sealer.
    async fn wait_for_all_commands(&mut self) {
        tracing::debug!(
            "Requested waiting for block persistence queue to empty; current available capacity: {}",
            self.commands_sender.capacity()
        );

        let start = Instant::now();
        let completion_receiver = self.latest_completion_receiver.take();
        if let Some(completion_receiver) = completion_receiver {
            completion_receiver.await.expect(Self::SHUTDOWN_MSG);
        }

        let elapsed = start.elapsed();
        tracing::debug!("Block persistence queue is emptied (took {elapsed:?})");

        // Since this method called from outside is essentially a no-op if `self.is_sync`,
        // we don't report its metrics in this case.
        if !self.is_sync {
            // L2_BLOCK_METRICS
            //     .seal_queue_capacity
            //     .set(self.commands_sender.capacity());
            // L2_BLOCK_METRICS.seal_queue_latency[&L2BlockQueueStage::WaitForAllCommands]
            //     .observe(elapsed);
        }
    }
}

#[async_trait]
impl StateKeeperOutputHandler for StateKeeperPersistence {
    async fn initialize(&mut self, cursor: &IoCursor) -> anyhow::Result<()> {
        let mut connection = self.pool.connection_tagged("state_keeper").await?;
        L2BlockSealProcess::clear_pending_l2_block(&mut connection, cursor.next_l2_block - 1).await
    }

    async fn handle_open_batch(&mut self, header: UnsealedL1BatchHeader) -> anyhow::Result<()> {
        self.submit_opened_batch(header).await;
        Ok(())
    }

    async fn handle_block(&mut self, finished_block: &FinishedBlock) -> anyhow::Result<()> {
        let command = BlockSealCommand {
            inner: finished_block.inner.clone(),
            initial_writes: finished_block.initial_writes.clone(),
            pre_insert_txs: self.pre_insert_txs,
        };
        self.submit_block(command).await;
        Ok(())
    }
}

/// A command together with the return address allowing to track command processing completion.
#[derive(Debug)]
struct Completable<T> {
    command: T,
    completion_sender: oneshot::Sender<()>,
}

#[derive(Debug)]
pub enum PersistenceCommand {
    OpenBatch(UnsealedL1BatchHeader),
    BlockSeal(BlockSealCommand),
}

impl PersistenceCommand {
    pub async fn process(self, pool: ConnectionPool<Core>) -> anyhow::Result<()> {
        match self {
            PersistenceCommand::OpenBatch(header) => {
                let mut conn = pool.connection_tagged("zk_os_state_keeper").await?;
                conn.blocks_dal().insert_l1_batch(header).await?;
            }
            PersistenceCommand::BlockSeal(command) => {
                command.seal(pool).await?;
            }
        }

        Ok(())
    }
}

/// Component responsible for storing block data to Postgres.
#[derive(Debug)]
pub struct BlockPersistenceTask {
    pool: ConnectionPool<Core>,
    is_sync: bool,
    // Weak sender handle to get queue capacity stats.
    commands_sender: mpsc::WeakSender<Completable<PersistenceCommand>>,
    commands_receiver: mpsc::Receiver<Completable<PersistenceCommand>>,
}

impl BlockPersistenceTask {
    pub async fn run(mut self) -> anyhow::Result<()> {
        if self.is_sync {
            tracing::info!("Starting synchronous block persistence");
        } else if let Some(sender) = self.commands_sender.upgrade() {
            tracing::info!(
                "Starting async block persistence with queue capacity {}",
                sender.max_capacity()
            );
        } else {
            tracing::warn!("Block persistence not started, since its handle is already dropped");
        }

        let mut l2_block_seal_delta: Option<Instant> = None;
        // Commands must be processed sequentially.
        while let Some(completable) = self.next_command().await {
            completable.command.process(self.pool.clone()).await?;
            // if let Some(delta) = l2_block_seal_delta {
            //     L2_BLOCK_METRICS.seal_delta.observe(delta.elapsed());
            // }
            l2_block_seal_delta = Some(Instant::now());

            completable.completion_sender.send(()).ok();
            // ^ We don't care whether anyone listens to the processing progress
        }
        Ok(())
    }

    async fn next_command(&mut self) -> Option<Completable<PersistenceCommand>> {
        tracing::debug!("Polling L2 block seal queue for next command");
        // let start = Instant::now();
        let command = self.commands_receiver.recv().await;
        // let elapsed = start.elapsed();

        // if let Some(completable) = &command {
        // tracing::debug!(
        //     "Received command to seal L2 block #{} (polling took {elapsed:?})",
        //     completable.command.l2_block.number
        // );
        // }

        // if !self.is_sync {
        //     L2_BLOCK_METRICS.seal_queue_latency[&L2BlockQueueStage::NextCommand].observe(elapsed);
        //     if let Some(sender) = self.commands_sender.upgrade() {
        //         L2_BLOCK_METRICS.seal_queue_capacity.set(sender.capacity());
        //     }
        // }
        command
    }
}
