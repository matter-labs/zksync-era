//! State keeper persistence logic.

use std::time::Instant;

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use zksync_dal::{ConnectionPool, Core};
use zksync_shared_metrics::{BlockStage, APP_METRICS};
use zksync_types::Address;

use crate::state_keeper::{
    io::StateKeeperOutputHandler,
    metrics::{L2BlockQueueStage, L2_BLOCK_METRICS},
    updates::{L2BlockSealCommand, UpdatesManager},
};

/// A command together with the return address allowing to track command processing completion.
#[derive(Debug)]
struct Completable<T> {
    command: T,
    completion_sender: oneshot::Sender<()>,
}

/// Canonical [`HandleStateKeeperOutput`] implementation that stores processed L2 blocks and L1 batches to Postgres.
#[derive(Debug)]
pub struct StateKeeperPersistence {
    pool: ConnectionPool<Core>,
    l2_shared_bridge_addr: Address,
    pre_insert_txs: bool,
    insert_protective_reads: bool,
    commands_sender: mpsc::Sender<Completable<L2BlockSealCommand>>,
    latest_completion_receiver: Option<oneshot::Receiver<()>>,
    // If true, `submit_l2_block()` will wait for the operation to complete.
    is_sync: bool,
}

impl StateKeeperPersistence {
    const SHUTDOWN_MSG: &'static str = "L2 block sealer unexpectedly shut down";

    /// Creates a sealer that will use the provided Postgres connection and will have the specified
    /// `command_capacity` for unprocessed sealing commands.
    pub fn new(
        pool: ConnectionPool<Core>,
        l2_shared_bridge_addr: Address,
        mut command_capacity: usize,
    ) -> (Self, L2BlockSealerTask) {
        let is_sync = command_capacity == 0;
        command_capacity = command_capacity.max(1);

        let (commands_sender, commands_receiver) = mpsc::channel(command_capacity);
        let sealer = L2BlockSealerTask {
            pool: pool.clone(),
            is_sync,
            commands_sender: commands_sender.downgrade(),
            commands_receiver,
        };
        let this = Self {
            pool,
            l2_shared_bridge_addr,
            pre_insert_txs: false,
            insert_protective_reads: true,
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

    /// Disables inserting protective reads to Postgres when persisting an L1 batch. This is only sound
    /// if the node won't *ever* run a full Merkle tree (such a tree requires protective reads to generate witness inputs).
    pub fn without_protective_reads(mut self) -> Self {
        self.insert_protective_reads = false;
        self
    }

    /// Submits a new sealing `command` to the sealer that this handle is attached to.
    ///
    /// If there are currently too many unprocessed commands, this method will wait until
    /// enough of them are processed (i.e., there is back pressure).
    async fn submit_l2_block(&mut self, command: L2BlockSealCommand) {
        let l2_block_number = command.l2_block.number;
        tracing::debug!(
            "Enqueuing sealing command for L2 block #{l2_block_number} with #{} txs (L1 batch #{})",
            command.l2_block.executed_transactions.len(),
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
            "Enqueued sealing command for L2 block #{l2_block_number} (took {elapsed:?}; \
             available queue capacity: {queue_capacity})"
        );

        if self.is_sync {
            self.wait_for_all_commands().await;
        } else {
            L2_BLOCK_METRICS.seal_queue_capacity.set(queue_capacity);
            L2_BLOCK_METRICS.seal_queue_latency[&L2BlockQueueStage::Submit].observe(elapsed);
        }
    }

    /// Waits until all previously submitted commands are fully processed by the sealer.
    async fn wait_for_all_commands(&mut self) {
        tracing::debug!(
            "Requested waiting for L2 block seal queue to empty; current available capacity: {}",
            self.commands_sender.capacity()
        );

        let start = Instant::now();
        let completion_receiver = self.latest_completion_receiver.take();
        if let Some(completion_receiver) = completion_receiver {
            completion_receiver.await.expect(Self::SHUTDOWN_MSG);
        }

        let elapsed = start.elapsed();
        tracing::debug!("L2 block seal queue is emptied (took {elapsed:?})");

        // Since this method called from outside is essentially a no-op if `self.is_sync`,
        // we don't report its metrics in this case.
        if !self.is_sync {
            L2_BLOCK_METRICS
                .seal_queue_capacity
                .set(self.commands_sender.capacity());
            L2_BLOCK_METRICS.seal_queue_latency[&L2BlockQueueStage::WaitForAllCommands]
                .observe(elapsed);
        }
    }
}

#[async_trait]
impl StateKeeperOutputHandler for StateKeeperPersistence {
    async fn handle_l2_block(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        let command =
            updates_manager.seal_l2_block_command(self.l2_shared_bridge_addr, self.pre_insert_txs);
        self.submit_l2_block(command).await;
        Ok(())
    }

    async fn handle_l1_batch(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        // We cannot start sealing an L1 batch until we've sealed all L2 blocks included in it.
        self.wait_for_all_commands().await;

        let pool = self.pool.clone();
        let mut storage = pool.connection_tagged("state_keeper").await?;
        let batch_number = updates_manager.l1_batch.number;
        updates_manager
            .seal_l1_batch(
                &mut storage,
                self.l2_shared_bridge_addr,
                self.insert_protective_reads,
            )
            .await
            .with_context(|| format!("cannot persist L1 batch #{batch_number}"))?;
        APP_METRICS.block_number[&BlockStage::Sealed].set(batch_number.0.into());
        Ok(())
    }
}

/// Component responsible for sealing L2 blocks (i.e., storing their data to Postgres).
#[derive(Debug)]
pub struct L2BlockSealerTask {
    pool: ConnectionPool<Core>,
    is_sync: bool,
    // Weak sender handle to get queue capacity stats.
    commands_sender: mpsc::WeakSender<Completable<L2BlockSealCommand>>,
    commands_receiver: mpsc::Receiver<Completable<L2BlockSealCommand>>,
}

impl L2BlockSealerTask {
    /// Seals L2 blocks as they are received from the [`StateKeeperPersistence`]. This should be run
    /// on a separate Tokio task.
    pub async fn run(mut self) -> anyhow::Result<()> {
        if self.is_sync {
            tracing::info!("Starting synchronous L2 block sealer");
        } else if let Some(sender) = self.commands_sender.upgrade() {
            tracing::info!(
                "Starting async L2 block sealer with queue capacity {}",
                sender.max_capacity()
            );
        } else {
            tracing::warn!("L2 block sealer not started, since its handle is already dropped");
        }

        let mut l2_block_seal_delta: Option<Instant> = None;
        // Commands must be processed sequentially: a later L2 block cannot be saved before
        // an earlier one.
        while let Some(completable) = self.next_command().await {
            let mut storage = self.pool.connection_tagged("state_keeper").await?;
            completable.command.seal(&mut storage).await?;
            if let Some(delta) = l2_block_seal_delta {
                L2_BLOCK_METRICS.seal_delta.observe(delta.elapsed());
            }
            l2_block_seal_delta = Some(Instant::now());

            completable.completion_sender.send(()).ok();
            // ^ We don't care whether anyone listens to the processing progress
        }
        Ok(())
    }

    async fn next_command(&mut self) -> Option<Completable<L2BlockSealCommand>> {
        tracing::debug!("Polling L2 block seal queue for next command");
        let start = Instant::now();
        let command = self.commands_receiver.recv().await;
        let elapsed = start.elapsed();

        if let Some(completable) = &command {
            tracing::debug!(
                "Received command to seal L2 block #{} (polling took {elapsed:?})",
                completable.command.l2_block.number
            );
        }

        if !self.is_sync {
            L2_BLOCK_METRICS.seal_queue_latency[&L2BlockQueueStage::NextCommand].observe(elapsed);
            if let Some(sender) = self.commands_sender.upgrade() {
                L2_BLOCK_METRICS.seal_queue_capacity.set(sender.capacity());
            }
        }
        command
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use assert_matches::assert_matches;
    use futures::FutureExt;
    use multivm::zk_evm_latest::ethereum_types::{H256, U256};
    use zksync_dal::CoreDal;
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_types::{
        api::TransactionStatus, block::BlockGasCount, tx::ExecutionMetrics, AccountTreeId,
        L1BatchNumber, L2BlockNumber, StorageKey, StorageLogQueryType,
    };
    use zksync_utils::u256_to_h256;

    use super::*;
    use crate::state_keeper::{
        io::L2BlockParams,
        tests::{
            create_execution_result, create_transaction, create_updates_manager,
            default_l1_batch_env, default_system_env, default_vm_batch_result, Query,
        },
    };

    async fn test_l2_block_and_l1_batch_processing(
        pool: ConnectionPool<Core>,
        l2_block_sealer_capacity: usize,
    ) {
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        // Save metadata for the genesis L1 batch so that we don't hang in `seal_l1_batch`.
        storage
            .blocks_dal()
            .set_l1_batch_hash(L1BatchNumber(0), H256::zero())
            .await
            .unwrap();
        drop(storage);

        let (mut persistence, l2_block_sealer) =
            StateKeeperPersistence::new(pool.clone(), Address::default(), l2_block_sealer_capacity);
        tokio::spawn(l2_block_sealer.run());
        execute_mock_batch(&mut persistence).await;

        // Check that L2 block #1 and L1 batch #1 are persisted.
        let mut storage = pool.connection().await.unwrap();
        assert_eq!(
            storage
                .blocks_dal()
                .get_sealed_l2_block_number()
                .await
                .unwrap(),
            Some(L2BlockNumber(2)) // + fictive L2 block
        );
        let l1_batch_header = storage
            .blocks_dal()
            .get_l1_batch_header(L1BatchNumber(1))
            .await
            .unwrap()
            .expect("No L1 batch #1");
        assert_eq!(l1_batch_header.l2_tx_count, 1);

        // Check that both initial writes and protective reads are persisted.
        let initial_writes = storage
            .storage_logs_dedup_dal()
            .dump_all_initial_writes_for_tests()
            .await;
        let initial_writes_in_last_batch = initial_writes
            .iter()
            .filter(|write| write.l1_batch_number == L1BatchNumber(1))
            .count();
        assert_eq!(initial_writes_in_last_batch, 1, "{initial_writes:?}");
        let protective_reads = storage
            .storage_logs_dedup_dal()
            .get_protective_reads_for_l1_batch(L1BatchNumber(1))
            .await
            .unwrap();
        assert_eq!(protective_reads.len(), 1, "{protective_reads:?}");
    }

    async fn execute_mock_batch(persistence: &mut StateKeeperPersistence) -> H256 {
        let l1_batch_env = default_l1_batch_env(1, 1, Address::random());
        let mut updates = UpdatesManager::new(&l1_batch_env, &default_system_env());

        let tx = create_transaction(10, 100);
        let tx_hash = tx.hash();
        let storage_logs = [
            (U256::from(1), Query::Read(U256::from(0))),
            (U256::from(2), Query::InitialWrite(U256::from(1))),
        ];
        let tx_result = create_execution_result(0, storage_logs);
        let storage_logs = tx_result.logs.storage_logs.clone();
        updates.extend_from_executed_transaction(
            tx,
            tx_result,
            vec![],
            BlockGasCount::default(),
            ExecutionMetrics::default(),
            vec![],
        );
        persistence.handle_l2_block(&updates).await.unwrap();
        updates.push_l2_block(L2BlockParams {
            timestamp: 1,
            virtual_blocks: 1,
        });

        let mut batch_result = default_vm_batch_result();
        batch_result
            .final_execution_state
            .deduplicated_storage_log_queries =
            storage_logs.iter().map(|query| query.log_query).collect();
        batch_result.initially_written_slots = Some(
            storage_logs
                .into_iter()
                .filter(|&log| log.log_type == StorageLogQueryType::InitialWrite)
                .map(|log| {
                    let key = StorageKey::new(
                        AccountTreeId::new(log.log_query.address),
                        u256_to_h256(log.log_query.key),
                    );
                    key.hashed_key()
                })
                .collect(),
        );

        updates.finish_batch(batch_result);
        persistence.handle_l1_batch(&updates).await.unwrap();

        tx_hash
    }

    #[tokio::test]
    async fn l2_block_and_l1_batch_processing() {
        let pool = ConnectionPool::constrained_test_pool(1).await;
        test_l2_block_and_l1_batch_processing(pool, 1).await;
    }

    #[tokio::test]
    async fn l2_block_and_l1_batch_processing_with_sync_sealer() {
        let pool = ConnectionPool::constrained_test_pool(1).await;
        test_l2_block_and_l1_batch_processing(pool, 0).await;
    }

    #[tokio::test]
    async fn l2_block_and_l1_batch_processing_on_full_node() {
        let pool = ConnectionPool::constrained_test_pool(1).await;
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        // Save metadata for the genesis L1 batch so that we don't hang in `seal_l1_batch`.
        storage
            .blocks_dal()
            .set_l1_batch_hash(L1BatchNumber(0), H256::zero())
            .await
            .unwrap();
        drop(storage);

        let (mut persistence, l2_block_sealer) =
            StateKeeperPersistence::new(pool.clone(), Address::default(), 1);
        persistence = persistence.with_tx_insertion().without_protective_reads();
        tokio::spawn(l2_block_sealer.run());

        let tx_hash = execute_mock_batch(&mut persistence).await;

        // Check that the transaction is persisted.
        let mut storage = pool.connection().await.unwrap();
        let tx_details = storage
            .transactions_web3_dal()
            .get_transaction_details(tx_hash)
            .await
            .unwrap()
            .expect("no transaction");
        assert_matches!(tx_details.status, TransactionStatus::Included);

        // Check that initial writes are persisted and protective reads are not.
        let initial_writes = storage
            .storage_logs_dedup_dal()
            .dump_all_initial_writes_for_tests()
            .await;
        let initial_writes_in_last_batch = initial_writes
            .iter()
            .filter(|write| write.l1_batch_number == L1BatchNumber(1))
            .count();
        assert_eq!(initial_writes_in_last_batch, 1, "{initial_writes:?}");
        let protective_reads = storage
            .storage_logs_dedup_dal()
            .get_protective_reads_for_l1_batch(L1BatchNumber(1))
            .await
            .unwrap();
        assert_eq!(protective_reads, HashSet::new());
    }

    #[tokio::test]
    async fn l2_block_sealer_handle_blocking() {
        let pool = ConnectionPool::constrained_test_pool(1).await;
        let (mut persistence, mut sealer) =
            StateKeeperPersistence::new(pool, Address::default(), 1);

        // The first command should be successfully submitted immediately.
        let mut updates_manager = create_updates_manager();
        let seal_command = updates_manager.seal_l2_block_command(Address::default(), false);
        persistence.submit_l2_block(seal_command).await;

        // The second command should lead to blocking
        updates_manager.push_l2_block(L2BlockParams {
            timestamp: 2,
            virtual_blocks: 1,
        });
        let seal_command = updates_manager.seal_l2_block_command(Address::default(), false);
        {
            let submit_future = persistence.submit_l2_block(seal_command);
            futures::pin_mut!(submit_future);

            assert!((&mut submit_future).now_or_never().is_none());
            // ...until L2 block #1 is processed
            let command = sealer.commands_receiver.recv().await.unwrap();
            command.completion_sender.send(()).unwrap_err(); // completion receiver should be dropped
            submit_future.await;
        }

        {
            let wait_future = persistence.wait_for_all_commands();
            futures::pin_mut!(wait_future);
            assert!((&mut wait_future).now_or_never().is_none());
            let command = sealer.commands_receiver.recv().await.unwrap();
            command.completion_sender.send(()).unwrap();
            wait_future.await;
        }

        // Check that `wait_for_all_commands()` state is reset after use.
        persistence.wait_for_all_commands().await;

        updates_manager.push_l2_block(L2BlockParams {
            timestamp: 3,
            virtual_blocks: 1,
        });
        let seal_command = updates_manager.seal_l2_block_command(Address::default(), false);
        persistence.submit_l2_block(seal_command).await;
        let command = sealer.commands_receiver.recv().await.unwrap();
        command.completion_sender.send(()).unwrap();
        persistence.wait_for_all_commands().await;
    }

    #[tokio::test]
    async fn l2_block_sealer_handle_parallel_processing() {
        let pool = ConnectionPool::constrained_test_pool(1).await;
        let (mut persistence, mut sealer) =
            StateKeeperPersistence::new(pool, Address::default(), 5);

        // 5 L2 block sealing commands can be submitted without blocking.
        let mut updates_manager = create_updates_manager();
        for i in 1..=5 {
            let seal_command = updates_manager.seal_l2_block_command(Address::default(), false);
            updates_manager.push_l2_block(L2BlockParams {
                timestamp: i,
                virtual_blocks: 1,
            });
            persistence.submit_l2_block(seal_command).await;
        }

        for i in 1..=5 {
            let command = sealer.commands_receiver.recv().await.unwrap();
            assert_eq!(command.command.l2_block.number, L2BlockNumber(i));
            command.completion_sender.send(()).ok();
        }

        persistence.wait_for_all_commands().await;
    }
}
