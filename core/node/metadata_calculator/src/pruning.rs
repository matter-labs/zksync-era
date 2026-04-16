//! Merkle tree pruning logic.

use std::time::Duration;

use anyhow::Context as _;
use serde::Serialize;
use tokio::sync::{oneshot, watch};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_merkle_tree::{MerkleTreePruner, MerkleTreePrunerHandle, RocksDBWrapper};
use zksync_types::L1BatchNumber;

pub(super) type PruningHandles = (MerkleTreePruner<RocksDBWrapper>, MerkleTreePrunerHandle);

#[derive(Debug, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
enum MerkleTreePruningTaskHealth {
    Initialization,
    Pruning {
        #[serde(skip_serializing_if = "Option::is_none")]
        target_retained_l1_batch_number: Option<L1BatchNumber>,
    },
    PruningStopped,
    ShuttingDown,
}

impl From<MerkleTreePruningTaskHealth> for Health {
    fn from(health: MerkleTreePruningTaskHealth) -> Self {
        let status = match &health {
            MerkleTreePruningTaskHealth::Initialization
            | MerkleTreePruningTaskHealth::PruningStopped => HealthStatus::Affected,
            MerkleTreePruningTaskHealth::Pruning { .. } => HealthStatus::Ready,
            MerkleTreePruningTaskHealth::ShuttingDown => HealthStatus::ShuttingDown,
        };
        Health::from(status).with_details(health)
    }
}

/// Task performing Merkle tree pruning according to the pruning entries in Postgres.
#[derive(Debug)]
#[must_use = "Task should `run()` in a managed Tokio task"]
pub struct MerkleTreePruningTask {
    handles: oneshot::Receiver<PruningHandles>,
    pool: ConnectionPool<Core>,
    health_updater: HealthUpdater,
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
            health_updater: ReactiveHealthCheck::new("tree_pruner").1,
            poll_interval,
        }
    }

    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        // The pruning task is "affected" (not functioning) until the Merkle tree is initialized.
        self.health_updater
            .update(MerkleTreePruningTaskHealth::Initialization.into());

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
                tracing::info!("Stop request received before Merkle tree is initialized; shutting down tree pruning");
                return Ok(());
            }
        }
        let health = MerkleTreePruningTaskHealth::Pruning {
            target_retained_l1_batch_number: None,
        };
        self.health_updater.update(health.into());
        tracing::info!("Obtained pruning handles; starting Merkle tree pruning");

        // Pruner is not allocated a managed task because it is blocking; its cancellation awareness inherently
        // depends on the pruner handle (i.e., this task).
        pruner.set_poll_interval(self.poll_interval);
        let pruner_task_handle = tokio::task::spawn_blocking(|| pruner.run());

        while !*stop_receiver.borrow_and_update() {
            let mut storage = self.pool.connection_tagged("metadata_calculator").await?;
            let pruning_info = storage.pruning_dal().get_pruning_info().await?;
            drop(storage);

            if let Some(pruned) = pruning_info.last_hard_pruned {
                let target_retained_l1_batch_number = pruned.l1_batch + 1;
                let target_retained_version = u64::from(target_retained_l1_batch_number.0);
                let Ok(prev_target_version) =
                    pruner_handle.set_target_retained_version(target_retained_version)
                else {
                    self.health_updater
                        .update(MerkleTreePruningTaskHealth::PruningStopped.into());
                    tracing::error!("Merkle tree pruning thread unexpectedly stopped");
                    return pruner_task_handle
                        .await
                        .context("Merkle tree pruning thread panicked")?;
                };

                if prev_target_version != target_retained_version {
                    let health = MerkleTreePruningTaskHealth::Pruning {
                        target_retained_l1_batch_number: Some(target_retained_l1_batch_number),
                    };
                    self.health_updater.update(health.into());
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

        self.health_updater
            .update(MerkleTreePruningTaskHealth::ShuttingDown.into());
        tracing::info!("Stop request received, Merkle tree pruning is shutting down");
        drop(pruner_handle);
        pruner_task_handle
            .await
            .context("Merkle tree pruning thread panicked")?
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use test_casing::test_casing;
    use zksync_node_genesis::{insert_genesis_batch, GenesisParamsInitials};
    use zksync_node_test_utils::prepare_recovery_snapshot;
    use zksync_types::{L1BatchNumber, L2BlockNumber, H256};

    use super::*;
    use crate::{
        tests::{extend_db_state_from_l1_batch, gen_storage_logs, mock_config, reset_db_state},
        MetadataCalculator,
    };

    const POLL_INTERVAL: Duration = Duration::from_millis(50);

    #[tokio::test]
    async fn basic_tree_pruning_workflow() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let config = mock_config(temp_dir.path());
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
            .await
            .unwrap();
        reset_db_state(&pool, 5).await;

        let mut calculator = MetadataCalculator::new(config, None, pool.clone())
            .await
            .unwrap();
        let reader = calculator.tree_reader();
        let pruning_task = calculator.pruning_task(POLL_INTERVAL);
        let mut health_check = pruning_task.health_check();
        let (stop_sender, stop_receiver) = watch::channel(false);
        let calculator_handle = tokio::spawn(calculator.run(stop_receiver.clone()));
        let pruning_task_handle = tokio::spawn(pruning_task.run(stop_receiver));

        health_check
            .wait_for(|health| matches!(health.status(), HealthStatus::Ready))
            .await;
        // Wait until the calculator is initialized.
        let reader = reader.wait().await.unwrap();
        while reader.clone().info().await.next_l1_batch_number < L1BatchNumber(6) {
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        // Add a pruning log to force pruning.
        storage
            .pruning_dal()
            .hard_prune_batches_range(L1BatchNumber(3), L2BlockNumber(3))
            .await
            .unwrap();
        storage
            .pruning_dal()
            .insert_hard_pruning_log(L1BatchNumber(3), L2BlockNumber(3), H256::zero())
            .await
            .unwrap();

        while reader.clone().info().await.min_l1_batch_number.unwrap() <= L1BatchNumber(3) {
            tokio::time::sleep(POLL_INTERVAL).await;
        }

        reader.verify_consistency(L1BatchNumber(5)).await.unwrap();

        stop_sender.send_replace(true);
        calculator_handle.await.unwrap().unwrap();
        pruning_task_handle.await.unwrap().unwrap();
        health_check
            .wait_for(|health| matches!(health.status(), HealthStatus::ShutDown))
            .await;
    }

    #[derive(Debug)]
    enum PrematureExitScenario {
        CalculatorDrop,
        StopSignal,
    }

    impl PrematureExitScenario {
        const ALL: [Self; 2] = [Self::CalculatorDrop, Self::StopSignal];
    }

    #[test_casing(2, PrematureExitScenario::ALL)]
    #[tokio::test]
    async fn pruning_task_premature_exit(scenario: PrematureExitScenario) {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let config = mock_config(temp_dir.path());
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
            .await
            .unwrap();

        let mut calculator = MetadataCalculator::new(config, None, pool.clone())
            .await
            .unwrap();
        let pruning_task = calculator.pruning_task(POLL_INTERVAL);
        let mut health_check = pruning_task.health_check();
        let (stop_sender, stop_receiver) = watch::channel(false);
        let pruning_task_handle = tokio::spawn(pruning_task.run(stop_receiver));

        // Task health should be set to "affected" until `calculator` is started (which is never).
        health_check
            .wait_for(|health| matches!(health.status(), HealthStatus::Affected))
            .await;

        match scenario {
            PrematureExitScenario::CalculatorDrop => drop(calculator),
            PrematureExitScenario::StopSignal => {
                stop_sender.send_replace(true);
            }
        }
        health_check
            .wait_for(|health| matches!(health.status(), HealthStatus::ShutDown))
            .await;
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

        let mut calculator = MetadataCalculator::new(config, None, pool.clone())
            .await
            .unwrap();
        let reader = calculator.tree_reader();
        let pruning_task = calculator.pruning_task(POLL_INTERVAL);
        let (stop_sender, stop_receiver) = watch::channel(false);
        let calculator_handle = tokio::spawn(calculator.run(stop_receiver.clone()));
        let pruning_task_handle = tokio::spawn(pruning_task.run(stop_receiver));

        // Wait until the calculator is initialized.
        let reader = reader.wait().await.unwrap();
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
                snapshot_recovery.l2_block_number + 1,
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
            .insert_hard_pruning_log(
                snapshot_recovery.l1_batch_number + 3,
                snapshot_recovery.l2_block_number + 3,
                H256::zero(), // not used
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
