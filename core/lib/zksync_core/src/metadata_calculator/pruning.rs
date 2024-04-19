//! Merkle tree pruning logic.

use std::time::Duration;

use anyhow::Context as _;
use tokio::sync::{oneshot, watch};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_merkle_tree::{MerkleTreePruner, MerkleTreePrunerHandle, RocksDBWrapper};

pub(super) type PruningHandles = (MerkleTreePruner<RocksDBWrapper>, MerkleTreePrunerHandle);

/// Task performing Merkle tree pruning according to the pruning entries in Postgres.
#[derive(Debug)]
#[must_use = "Task should `run()` in a managed Tokio task"]
pub struct MerkleTreePruningTask {
    handles: oneshot::Receiver<PruningHandles>,
    pool: ConnectionPool<Core>,
    poll_interval: Duration,
}

impl MerkleTreePruningTask {
    pub(super) fn new(
        handles: oneshot::Receiver<PruningHandles>,
        pool: ConnectionPool<Core>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            handles,
            pool,
            poll_interval,
        }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let (mut pruner, pruner_handle);
        tokio::select! {
            res = self.handles => {
                match res {
                    Ok(handles) => (pruner, pruner_handle) = handles,
                    Err(_) => {
                        tracing::info!("Merkle tree dropped; shutting down tree pruning");
                        return Ok(());
                    }
                }
            }
            _ = stop_receiver.changed() => {
                tracing::info!("Stop signal received before Merkle tree is initialized; shutting down tree pruning");
                return Ok(());
            }
        }
        tracing::info!("Obtained pruning handles; starting Merkle tree pruning");

        // FIXME: is this good enough (vs a managed task)?
        pruner.set_poll_interval(self.poll_interval);
        let pruner_task_handle = tokio::task::spawn_blocking(|| pruner.run());

        while !*stop_receiver.borrow_and_update() {
            let mut storage = self.pool.connection_tagged("metadata_calculator").await?;
            let pruning_info = storage.pruning_dal().get_pruning_info().await?;
            drop(storage);

            if let Some(l1_batch_number) = pruning_info.last_hard_pruned_l1_batch {
                let target_retained_version = u64::from(l1_batch_number.0) + 1;
                let prev_target_version =
                    pruner_handle.set_target_retained_version(target_retained_version);
                if prev_target_version != target_retained_version {
                    tracing::info!("Set target retained tree version from {prev_target_version} to {target_retained_version}");
                }
            }

            if tokio::time::timeout(self.poll_interval, stop_receiver.changed())
                .await
                .is_ok()
            {
                break;
            }
        }

        tracing::info!("Stop signal received, Merkle tree pruning is shutting down");
        pruner_handle.abort();
        pruner_task_handle
            .await
            .context("Merkle tree pruning thread panicked")
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use zksync_types::{L1BatchNumber, L2BlockNumber};

    use super::*;
    use crate::{
        genesis::{insert_genesis_batch, GenesisParams},
        metadata_calculator::{
            tests::{extend_db_state_from_l1_batch, gen_storage_logs, mock_config, reset_db_state},
            MetadataCalculator,
        },
        utils::testonly::prepare_recovery_snapshot,
    };

    const POLL_INTERVAL: Duration = Duration::from_millis(50);

    #[tokio::test]
    async fn basic_tree_pruning_workflow() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let config = mock_config(temp_dir.path());
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParams::mock())
            .await
            .unwrap();
        reset_db_state(&pool, 5).await;

        let mut calculator = MetadataCalculator::new(config, None, pool.clone(), pool.clone())
            .await
            .unwrap();
        let reader = calculator.tree_reader();
        let pruning_task = calculator.pruning_task(POLL_INTERVAL);
        let (stop_sender, stop_receiver) = watch::channel(false);
        let calculator_handle = tokio::spawn(calculator.run(stop_receiver.clone()));
        let pruning_task_handle = tokio::spawn(pruning_task.run(stop_receiver));

        // Wait until the calculator is initialized.
        let reader = reader.wait().await;
        while reader.clone().info().await.next_l1_batch_number < L1BatchNumber(6) {
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        // Add a pruning log to force pruning.
        storage
            .pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(3), L2BlockNumber(3))
            .await
            .unwrap();

        while reader.clone().info().await.min_l1_batch_number.unwrap() <= L1BatchNumber(3) {
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        reader.verify_consistency(L1BatchNumber(5)).await.unwrap();

        stop_sender.send_replace(true);
        calculator_handle.await.unwrap().unwrap();
        pruning_task_handle.await.unwrap().unwrap();
    }

    #[tokio::test]
    async fn pruning_after_snapshot_recovery() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let snapshot_logs = gen_storage_logs(100..300, 1).pop().unwrap();
        let mut storage = pool.connection().await.unwrap();
        let snapshot_recovery = prepare_recovery_snapshot(
            &mut storage,
            L1BatchNumber(23),
            L2BlockNumber(23),
            &snapshot_logs,
        )
        .await;

        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let config = mock_config(temp_dir.path());

        let mut calculator = MetadataCalculator::new(config, None, pool.clone(), pool.clone())
            .await
            .unwrap();
        let reader = calculator.tree_reader();
        let pruning_task = calculator.pruning_task(POLL_INTERVAL);
        let (stop_sender, stop_receiver) = watch::channel(false);
        let calculator_handle = tokio::spawn(calculator.run(stop_receiver.clone()));
        let pruning_task_handle = tokio::spawn(pruning_task.run(stop_receiver));

        // Wait until the calculator is initialized.
        let reader = reader.wait().await;
        let tree_info = reader.clone().info().await;
        assert_eq!(
            tree_info.next_l1_batch_number,
            snapshot_recovery.l1_batch_number + 1
        );
        assert_eq!(
            tree_info.min_l1_batch_number,
            Some(snapshot_recovery.l1_batch_number)
        );

        // Add some new L1 batches and wait for them to be processed.
        let mut new_logs = gen_storage_logs(500..600, 5);
        {
            let mut storage = storage.start_transaction().await.unwrap();
            // Logs must be sorted by `log.key` to match their enum index assignment
            for batch_logs in &mut new_logs {
                batch_logs.sort_unstable_by_key(|log| log.key);
            }

            extend_db_state_from_l1_batch(
                &mut storage,
                snapshot_recovery.l1_batch_number + 1,
                new_logs,
            )
            .await;
            storage.commit().await.unwrap();
        }

        let original_tree_info = loop {
            let tree_info = reader.clone().info().await;
            if tree_info.next_l1_batch_number == snapshot_recovery.l1_batch_number + 6 {
                break tree_info;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        };

        // Prune first 3 created batches in Postgres.
        storage
            .pruning_dal()
            .hard_prune_batches_range(
                snapshot_recovery.l1_batch_number + 3,
                snapshot_recovery.l2_block_number + 3,
            )
            .await
            .unwrap();

        // Check that the batches are pruned in the tree.
        let pruned_tree_info = loop {
            let tree_info = reader.clone().info().await;
            if tree_info.min_l1_batch_number.unwrap() > snapshot_recovery.l1_batch_number + 3 {
                break tree_info;
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        };

        assert_eq!(pruned_tree_info.root_hash, original_tree_info.root_hash);
        assert_eq!(pruned_tree_info.leaf_count, original_tree_info.leaf_count);
        reader
            .verify_consistency(pruned_tree_info.next_l1_batch_number - 1)
            .await
            .unwrap();

        stop_sender.send_replace(true);
        calculator_handle.await.unwrap().unwrap();
        pruning_task_handle.await.unwrap().unwrap();
    }
}
