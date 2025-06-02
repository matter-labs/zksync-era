//! State keeper persistence logic.

use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_shared_metrics::{BlockStage, APP_METRICS};
use zksync_types::{
    block::L2BlockHeader, u256_to_h256, writes::TreeWrite, Address, L2BlockNumber,
    ProtocolVersionId,
};

use crate::{
    io::{seal_logic::l2_block_seal_subtasks::L2BlockSealProcess, StateKeeperOutputHandler},
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
    l2_legacy_shared_bridge_addr: Option<Address>,
    pre_insert_txs: bool,
    insert_protective_reads: bool,
    commands_sender: mpsc::Sender<Completable<L2BlockSealCommand>>,
    latest_completion_receiver: Option<oneshot::Receiver<()>>,
    // If true, `submit_l2_block()` will wait for the operation to complete.
    is_sync: bool,
}

impl StateKeeperPersistence {
    const SHUTDOWN_MSG: &'static str = "L2 block sealer unexpectedly shut down";

    async fn validate_l2_legacy_shared_bridge_addr(
        pool: &ConnectionPool<Core>,
        l2_legacy_shared_bridge_addr: Option<Address>,
    ) -> anyhow::Result<()> {
        let mut connection = pool.connection_tagged("state_keeper").await?;

        if let Some(l2_block) = connection
            .blocks_dal()
            .get_earliest_l2_block_number()
            .await
            .context("failed to load earliest l2 block number")?
        {
            let header = connection
                .blocks_dal()
                .get_l2_block_header(l2_block)
                .await
                .context("failed to load L2 block header")?
                .context("missing L2 block header")?;
            let protocol_version = header
                .protocol_version
                .unwrap_or_else(ProtocolVersionId::last_potentially_undefined);

            if protocol_version.is_pre_gateway() && l2_legacy_shared_bridge_addr.is_none() {
                anyhow::bail!("Missing `l2_legacy_shared_bridge_addr` for chain that was initialized before gateway upgrade");
            }
        }

        Ok(())
    }

    /// Creates a sealer that will use the provided Postgres connection and will have the specified
    /// `command_capacity` for unprocessed sealing commands.
    pub async fn new(
        pool: ConnectionPool<Core>,
        l2_legacy_shared_bridge_addr: Option<Address>,
        mut command_capacity: usize,
    ) -> anyhow::Result<(Self, L2BlockSealerTask)> {
        Self::validate_l2_legacy_shared_bridge_addr(&pool, l2_legacy_shared_bridge_addr).await?;

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
            l2_legacy_shared_bridge_addr,
            pre_insert_txs: false,
            insert_protective_reads: true,
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
        let command = updates_manager
            .seal_l2_block_command(self.l2_legacy_shared_bridge_addr, self.pre_insert_txs);
        self.submit_l2_block(command).await;
        Ok(())
    }

    async fn handle_l2_block_header(&mut self, header: &L2BlockHeader) -> anyhow::Result<()> {
        let mut conn = self.pool.connection_tagged("state_keeper").await?;
        conn.blocks_dal().insert_l2_block(header).await?;
        Ok(())
    }

    async fn rollback_pending_l2_block(
        &mut self,
        l2_block_to_rollback: L2BlockNumber,
    ) -> anyhow::Result<()> {
        // We cannot start rollback before block data is sealed fully.
        self.wait_for_all_commands().await;

        let mut conn = self.pool.connection_tagged("state_keeper").await?;
        L2BlockSealProcess::clear_pending_l2_block(&mut conn, l2_block_to_rollback - 1).await?;
        Ok(())
    }

    async fn handle_l1_batch(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        // We cannot start sealing an L1 batch until we've sealed all L2 blocks included in it.
        self.wait_for_all_commands().await;

        let batch_number = updates_manager.l1_batch.number;
        updates_manager
            .seal_l1_batch(
                self.pool.clone(),
                self.l2_legacy_shared_bridge_addr,
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
            completable.command.seal(self.pool.clone()).await?;
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

/// Stores tree writes for L1 batches to Postgres.
/// It is expected to be run after `StateKeeperPersistence` as it appends data to `l1_batches` table.
#[derive(Debug)]
pub struct TreeWritesPersistence {
    pool: ConnectionPool<Core>,
}

impl TreeWritesPersistence {
    pub fn new(pool: ConnectionPool<Core>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl StateKeeperOutputHandler for TreeWritesPersistence {
    async fn handle_l2_block(&mut self, _updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        Ok(())
    }

    async fn handle_l1_batch(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        let mut connection = self.pool.connection_tagged("state_keeper").await?;
        let finished_batch = updates_manager
            .l1_batch
            .finished
            .as_ref()
            .context("L1 batch is not actually finished")?;

        let mut next_index = connection
            .storage_logs_dedup_dal()
            .max_enumeration_index_by_l1_batch(updates_manager.l1_batch.number - 1)
            .await?
            .unwrap_or(0)
            + 1;
        let tree_input: Vec<_> = if let Some(state_diffs) = &finished_batch.state_diffs {
            state_diffs
                .iter()
                .map(|diff| {
                    let leaf_index = if diff.is_write_initial() {
                        next_index += 1;
                        next_index - 1
                    } else {
                        diff.enumeration_index
                    };
                    TreeWrite {
                        address: diff.address,
                        key: u256_to_h256(diff.key),
                        value: u256_to_h256(diff.final_value),
                        leaf_index,
                    }
                })
                .collect()
        } else {
            let deduplicated_writes = finished_batch
                .final_execution_state
                .deduplicated_storage_logs
                .iter()
                .filter(|log_query| log_query.is_write());
            let deduplicated_writes_hashed_keys: Vec<_> = deduplicated_writes
                .clone()
                .map(|log| log.key.hashed_key())
                .collect();
            let non_initial_writes = connection
                .storage_logs_dal()
                .get_l1_batches_and_indices_for_initial_writes(&deduplicated_writes_hashed_keys)
                .await?;
            deduplicated_writes
                .map(|log| {
                    let leaf_index = if let Some((_, leaf_index)) =
                        non_initial_writes.get(&log.key.hashed_key())
                    {
                        *leaf_index
                    } else {
                        next_index += 1;
                        next_index - 1
                    };
                    TreeWrite {
                        address: *log.key.address(),
                        key: *log.key.key(),
                        value: log.value,
                        leaf_index,
                    }
                })
                .collect()
        };

        connection
            .blocks_dal()
            .set_tree_writes(updates_manager.l1_batch.number, tree_input)
            .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use assert_matches::assert_matches;
    use futures::FutureExt;
    use zksync_dal::CoreDal;
    use zksync_multivm::interface::{FinishedL1Batch, VmExecutionMetrics};
    use zksync_node_genesis::{insert_genesis_batch, GenesisParams};
    use zksync_node_test_utils::{default_l1_batch_env, default_system_env};
    use zksync_types::{
        api::TransactionStatus, h256_to_u256, writes::StateDiffRecord, L1BatchNumber,
        L2BlockNumber, StorageLogKind, H256, U256,
    };

    use super::*;
    use crate::{
        io::L2BlockParams,
        tests::{create_execution_result, create_transaction, create_updates_manager, Query},
        OutputHandler,
    };

    async fn test_l2_block_and_l1_batch_processing(
        pool: ConnectionPool<Core>,
        l2_block_sealer_capacity: usize,
    ) {
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        let initial_writes_in_genesis_batch = storage
            .storage_logs_dedup_dal()
            .max_enumeration_index_by_l1_batch(L1BatchNumber(0))
            .await
            .unwrap()
            .unwrap();
        // Save metadata for the genesis L1 batch so that we don't hang in `seal_l1_batch`.
        storage
            .blocks_dal()
            .set_l1_batch_hash(L1BatchNumber(0), H256::zero())
            .await
            .unwrap();
        drop(storage);

        let (persistence, l2_block_sealer) = StateKeeperPersistence::new(
            pool.clone(),
            Some(Address::default()),
            l2_block_sealer_capacity,
        )
        .await
        .unwrap();
        let mut output_handler = OutputHandler::new(Box::new(persistence))
            .with_handler(Box::new(TreeWritesPersistence::new(pool.clone())));
        tokio::spawn(l2_block_sealer.run());
        execute_mock_batch(&mut output_handler, &pool).await;

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
        let tree_writes = storage
            .blocks_dal()
            .get_tree_writes(L1BatchNumber(1))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(tree_writes.len(), 1, "{tree_writes:?}");
        // This write is initial and should have the next index.
        let actual_index = tree_writes[0].leaf_index;
        let expected_index = initial_writes_in_genesis_batch + 1;
        assert_eq!(actual_index, expected_index);
    }

    async fn execute_mock_batch(
        output_handler: &mut OutputHandler,
        pool: &ConnectionPool<Core>,
    ) -> H256 {
        let l1_batch_env = default_l1_batch_env(1, 1, Address::random());
        let mut updates = UpdatesManager::new(
            &l1_batch_env,
            &default_system_env(),
            Default::default(),
            Default::default(),
            None,
        );
        pool.connection()
            .await
            .unwrap()
            .blocks_dal()
            .insert_l1_batch(l1_batch_env.into_unsealed_header(None))
            .await
            .unwrap();

        let tx = create_transaction(10, 100);
        let tx_hash = tx.hash();
        let storage_logs = [
            (U256::from(1), Query::Read(U256::from(0))),
            (U256::from(2), Query::InitialWrite(U256::from(1))),
        ];
        let tx_result = create_execution_result(storage_logs);
        let storage_logs = tx_result.logs.storage_logs.clone();
        updates.extend_from_executed_transaction(
            tx,
            tx_result,
            VmExecutionMetrics::default(),
            vec![],
        );
        output_handler.handle_l2_block(&updates).await.unwrap();
        updates.set_next_l2_block_params(L2BlockParams {
            timestamp: 1,
            virtual_blocks: 1,
        });
        updates.push_l2_block();

        let mut batch_result = FinishedL1Batch::mock();
        batch_result.final_execution_state.deduplicated_storage_logs =
            storage_logs.iter().map(|log| log.log).collect();
        batch_result.state_diffs = Some(
            storage_logs
                .into_iter()
                .filter(|&log| log.log.kind == StorageLogKind::InitialWrite)
                .map(|log| StateDiffRecord {
                    address: *log.log.key.address(),
                    key: h256_to_u256(*log.log.key.key()),
                    derived_key: log.log.key.hashed_key().0,
                    enumeration_index: 0,
                    initial_value: h256_to_u256(log.previous_value),
                    final_value: h256_to_u256(log.log.value),
                })
                .collect(),
        );

        updates.finish_batch(batch_result);
        output_handler
            .handle_l1_batch(Arc::new(updates))
            .await
            .unwrap();

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
            StateKeeperPersistence::new(pool.clone(), Some(Address::default()), 1)
                .await
                .unwrap();
        persistence = persistence.with_tx_insertion().without_protective_reads();
        let mut output_handler = OutputHandler::new(Box::new(persistence));
        tokio::spawn(l2_block_sealer.run());

        let tx_hash = execute_mock_batch(&mut output_handler, &pool).await;

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
            StateKeeperPersistence::new(pool, Some(Address::default()), 1)
                .await
                .unwrap();

        // The first command should be successfully submitted immediately.
        let mut updates_manager = create_updates_manager();
        let seal_command = updates_manager.seal_l2_block_command(Some(Address::default()), false);
        persistence.submit_l2_block(seal_command).await;

        // The second command should lead to blocking
        updates_manager.set_next_l2_block_params(L2BlockParams {
            timestamp: 2,
            virtual_blocks: 1,
        });
        updates_manager.push_l2_block();
        let seal_command = updates_manager.seal_l2_block_command(Some(Address::default()), false);
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

        updates_manager.set_next_l2_block_params(L2BlockParams {
            timestamp: 3,
            virtual_blocks: 1,
        });
        updates_manager.push_l2_block();
        let seal_command = updates_manager.seal_l2_block_command(Some(Address::default()), false);
        persistence.submit_l2_block(seal_command).await;
        let command = sealer.commands_receiver.recv().await.unwrap();
        command.completion_sender.send(()).unwrap();
        persistence.wait_for_all_commands().await;
    }

    #[tokio::test]
    async fn l2_block_sealer_handle_parallel_processing() {
        let pool = ConnectionPool::constrained_test_pool(1).await;
        let (mut persistence, mut sealer) =
            StateKeeperPersistence::new(pool, Some(Address::default()), 5)
                .await
                .unwrap();

        // 5 L2 block sealing commands can be submitted without blocking.
        let mut updates_manager = create_updates_manager();
        for i in 1..=5 {
            let seal_command =
                updates_manager.seal_l2_block_command(Some(Address::default()), false);
            updates_manager.set_next_l2_block_params(L2BlockParams {
                timestamp: i,
                virtual_blocks: 1,
            });
            updates_manager.push_l2_block();
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
