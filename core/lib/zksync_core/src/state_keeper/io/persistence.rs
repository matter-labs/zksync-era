//! State keeper persistence logic.

use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use multivm::interface::{FinishedL1Batch, L1BatchEnv};
use tokio::sync::{mpsc, oneshot};
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStore;
use zksync_types::{witness_block_state::WitnessBlockState, Address};

use crate::state_keeper::{
    io::HandleStateKeeperOutput,
    metrics::{MiniblockQueueStage, MINIBLOCK_METRICS},
    updates::{MiniblockSealCommand, UpdatesManager},
};

/// A command together with the return address allowing to track command processing completion.
#[derive(Debug)]
struct Completable<T> {
    command: T,
    completion_sender: oneshot::Sender<()>,
}

/// Canonical [`HandleStateKeeperOutput`] implementation that stores processed miniblocks and L1 batches to Postgres.
#[derive(Debug)]
pub struct StateKeeperPersistence {
    pool: ConnectionPool,
    object_store: Option<Arc<dyn ObjectStore>>, // FIXME: split off?
    l2_erc20_bridge_addr: Address,
    pre_insert_txs: bool,
    commands_sender: mpsc::Sender<Completable<MiniblockSealCommand>>,
    latest_completion_receiver: Option<oneshot::Receiver<()>>,
    // If true, `submit_miniblock()` will wait for the operation to complete.
    is_sync: bool,
}

impl StateKeeperPersistence {
    const SHUTDOWN_MSG: &'static str = "miniblock sealer unexpectedly shut down";

    /// Creates a sealer that will use the provided Postgres connection and will have the specified
    /// `command_capacity` for unprocessed sealing commands.
    pub fn new(
        pool: ConnectionPool,
        l2_erc20_bridge_addr: Address,
        mut command_capacity: usize,
    ) -> (Self, MiniblockSealerTask) {
        let is_sync = command_capacity == 0;
        command_capacity = command_capacity.max(1);

        let (commands_sender, commands_receiver) = mpsc::channel(command_capacity);
        let sealer = MiniblockSealerTask {
            pool: pool.clone(),
            is_sync,
            commands_sender: commands_sender.downgrade(),
            commands_receiver,
        };
        let this = Self {
            pool,
            object_store: None,
            l2_erc20_bridge_addr,
            pre_insert_txs: false,
            commands_sender,
            latest_completion_receiver: None,
            is_sync,
        };
        (this, sealer)
    }

    pub fn with_tx_insertion(mut self) -> Self {
        self.pre_insert_txs = true;
        self
    }

    pub fn with_object_store(mut self, object_store: Arc<dyn ObjectStore>) -> Self {
        self.object_store = Some(object_store);
        self
    }

    /// Submits a new sealing `command` to the sealer that this handle is attached to.
    ///
    /// If there are currently too many unprocessed commands, this method will wait until
    /// enough of them are processed (i.e., there is back pressure).
    async fn submit_miniblock(&mut self, command: MiniblockSealCommand) {
        let miniblock_number = command.miniblock.number;
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
    async fn wait_for_all_commands(&mut self) {
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

#[async_trait]
impl HandleStateKeeperOutput for StateKeeperPersistence {
    async fn handle_miniblock(&mut self, updates_manager: &UpdatesManager) {
        let command =
            updates_manager.seal_miniblock_command(self.l2_erc20_bridge_addr, self.pre_insert_txs);
        self.submit_miniblock(command).await;
    }

    async fn handle_l1_batch(
        &mut self,
        witness_block_state: Option<WitnessBlockState>,
        updates_manager: UpdatesManager,
        l1_batch_env: &L1BatchEnv,
        finished_batch: FinishedL1Batch,
    ) -> anyhow::Result<()> {
        assert_eq!(
            updates_manager.batch_timestamp(),
            l1_batch_env.timestamp,
            "Batch timestamps don't match, batch number {}",
            l1_batch_env.number
        );

        // We cannot start sealing an L1 batch until we've sealed all miniblocks included in it.
        self.wait_for_all_commands().await;

        if let Some(witness_witness_block_state) = witness_block_state {
            let store = self
                .object_store
                .as_deref()
                .context("object store not set when saving `WitnessBlockState`")?;
            match store
                .put(l1_batch_env.number, &witness_witness_block_state)
                .await
            {
                Ok(path) => {
                    tracing::debug!("Successfully uploaded witness block start state to Object Store to path = '{path}'");
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to upload witness block start state to Object Store: {e:?}"
                    );
                }
            }
        }

        let pool = self.pool.clone();
        let mut storage = pool.access_storage_tagged("state_keeper").await?;
        updates_manager
            .seal_l1_batch(
                &mut storage,
                l1_batch_env,
                finished_batch,
                self.l2_erc20_bridge_addr,
            )
            .await;
        Ok(())
    }
}

/// Component responsible for sealing miniblocks (i.e., storing their data to Postgres).
#[derive(Debug)]
pub struct MiniblockSealerTask {
    pool: ConnectionPool,
    is_sync: bool,
    // Weak sender handle to get queue capacity stats.
    commands_sender: mpsc::WeakSender<Completable<MiniblockSealCommand>>,
    commands_receiver: mpsc::Receiver<Completable<MiniblockSealCommand>>,
}

impl MiniblockSealerTask {
    /// Seals miniblocks as they are received from the [`StateKeeperPersistence`]. This should be run
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
                completable.command.miniblock.number
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
