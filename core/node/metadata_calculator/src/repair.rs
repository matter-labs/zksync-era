//! High-level wrapper for the stale keys repair task.

use std::{
    sync::{Arc, Weak},
    time::Duration,
};

use anyhow::Context as _;
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use serde::Serialize;
use tokio::sync::watch;
use zksync_health_check::{CheckHealth, Health, HealthStatus};
use zksync_merkle_tree::repair;

use crate::LazyAsyncTreeReader;

#[derive(Debug, Serialize)]
struct RepairHealthDetails {
    #[serde(skip_serializing_if = "Option::is_none")]
    earliest_checked_version: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_checked_version: Option<u64>,
    repaired_key_count: usize,
}

impl From<repair::StaleKeysRepairStats> for RepairHealthDetails {
    fn from(stats: repair::StaleKeysRepairStats) -> Self {
        let versions = stats.checked_versions.as_ref();
        Self {
            earliest_checked_version: versions.map(|versions| *versions.start()),
            latest_checked_version: versions.map(|versions| *versions.end()),
            repaired_key_count: stats.repaired_key_count,
        }
    }
}

#[derive(Debug, Default)]
struct RepairHealthCheck {
    handle: OnceCell<Weak<repair::StaleKeysRepairHandle>>,
}

#[async_trait]
impl CheckHealth for RepairHealthCheck {
    fn name(&self) -> &'static str {
        "tree_stale_keys_repair"
    }

    async fn check_health(&self) -> Health {
        let Some(weak_handle) = self.handle.get() else {
            return HealthStatus::Affected.into();
        };
        let Some(handle) = weak_handle.upgrade() else {
            return HealthStatus::ShutDown.into();
        };
        Health::from(HealthStatus::Ready).with_details(RepairHealthDetails::from(handle.stats()))
    }
}

/// Stale keys repair task.
#[derive(Debug)]
#[must_use = "Task should `run()` in a managed Tokio task"]
pub struct StaleKeysRepairTask {
    tree_reader: LazyAsyncTreeReader,
    health_check: Arc<RepairHealthCheck>,
    poll_interval: Duration,
}

impl StaleKeysRepairTask {
    pub(super) fn new(tree_reader: LazyAsyncTreeReader) -> Self {
        Self {
            tree_reader,
            health_check: Arc::default(),
            poll_interval: Duration::from_secs(60),
        }
    }

    pub fn health_check(&self) -> Arc<dyn CheckHealth> {
        self.health_check.clone()
    }

    /// Runs this task indefinitely.
    #[tracing::instrument(skip_all)]
    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let db = tokio::select! {
            res = self.tree_reader.wait() => {
                match res {
                    Some(reader) => reader.into_db(),
                    None => {
                        tracing::info!("Merkle tree dropped; shutting down stale keys repair");
                        return Ok(());
                    }
                }
            }
            _ = stop_receiver.changed() => {
                tracing::info!("Stop request received before Merkle tree is initialized; shutting down stale keys repair");
                return Ok(());
            }
        };

        let (mut task, handle) = repair::StaleKeysRepairTask::new(db);
        task.set_poll_interval(self.poll_interval);
        let handle = Arc::new(handle);
        self.health_check
            .handle
            .set(Arc::downgrade(&handle))
            .map_err(|_| anyhow::anyhow!("failed setting health check handle"))?;

        let mut task = tokio::task::spawn_blocking(|| task.run());
        tokio::select! {
            res = &mut task => {
                tracing::error!("Stale keys repair spontaneously stopped");
                res.context("repair task panicked")?
            },
            _ = stop_receiver.changed() => {
                tracing::info!("Stop request received, stale keys repair is shutting down");
                // This is the only strong reference to the handle, so dropping it should signal the task to stop.
                drop(handle);
                task.await.context("stale keys repair task panicked")?
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tempfile::TempDir;
    use zksync_dal::{ConnectionPool, Core};
    use zksync_node_genesis::{insert_genesis_batch, GenesisParamsInitials};
    use zksync_types::L1BatchNumber;

    use super::*;
    use crate::{
        tests::{extend_db_state, gen_storage_logs, mock_config, reset_db_state},
        MetadataCalculator,
    };

    const POLL_INTERVAL: Duration = Duration::from_millis(50);

    async fn wait_for_health(
        check: &dyn CheckHealth,
        mut condition: impl FnMut(&Health) -> bool,
    ) -> Health {
        loop {
            let health = check.check_health().await;
            if condition(&health) {
                return health;
            } else if matches!(
                health.status(),
                HealthStatus::ShutDown | HealthStatus::Panicked
            ) {
                panic!("reached terminal health: {health:?}");
            }
            tokio::time::sleep(POLL_INTERVAL).await;
        }
    }

    #[tokio::test]
    async fn repair_task_basics() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let temp_dir = TempDir::new().expect("failed get temporary directory for RocksDB");
        let config = mock_config(temp_dir.path());
        let mut storage = pool.connection().await.unwrap();
        insert_genesis_batch(&mut storage, &GenesisParamsInitials::mock())
            .await
            .unwrap();
        reset_db_state(&pool, 5).await;

        let calculator = MetadataCalculator::new(config, None, pool.clone())
            .await
            .unwrap();
        let reader = calculator.tree_reader();
        let mut repair_task = calculator.stale_keys_repair_task();
        repair_task.poll_interval = POLL_INTERVAL;
        let health_check = repair_task.health_check();

        let (stop_sender, stop_receiver) = watch::channel(false);
        let calculator_handle = tokio::spawn(calculator.run(stop_receiver.clone()));
        let repair_task_handle = tokio::spawn(repair_task.run(stop_receiver));
        wait_for_health(&health_check, |health| {
            matches!(health.status(), HealthStatus::Ready)
        })
        .await;

        // Wait until the calculator is initialized and then drop the reader so that it doesn't lock RocksDB.
        {
            let reader = reader.wait().await.unwrap();
            while reader.clone().info().await.next_l1_batch_number < L1BatchNumber(6) {
                tokio::time::sleep(POLL_INTERVAL).await;
            }
        }

        // Wait until all tree versions have been checked.
        let health = wait_for_health(&health_check, |health| {
            if !matches!(health.status(), HealthStatus::Ready) {
                return false;
            }
            let details = health.details().unwrap();
            details.get("latest_checked_version") == Some(&5.into())
        })
        .await;
        let details = health.details().unwrap();
        assert_eq!(details["earliest_checked_version"], 1);
        assert_eq!(details["repaired_key_count"], 0);

        stop_sender.send_replace(true);
        calculator_handle.await.unwrap().unwrap();
        repair_task_handle.await.unwrap().unwrap();
        wait_for_health(&health_check, |health| {
            matches!(health.status(), HealthStatus::ShutDown)
        })
        .await;

        test_repair_persistence(temp_dir, pool).await;
    }

    async fn test_repair_persistence(temp_dir: TempDir, pool: ConnectionPool<Core>) {
        let config = mock_config(temp_dir.path());
        let calculator = MetadataCalculator::new(config, None, pool.clone())
            .await
            .unwrap();
        let mut repair_task = calculator.stale_keys_repair_task();
        repair_task.poll_interval = POLL_INTERVAL;
        let health_check = repair_task.health_check();

        let (stop_sender, stop_receiver) = watch::channel(false);
        let calculator_handle = tokio::spawn(calculator.run(stop_receiver.clone()));
        let repair_task_handle = tokio::spawn(repair_task.run(stop_receiver));
        wait_for_health(&health_check, |health| {
            matches!(health.status(), HealthStatus::Ready)
        })
        .await;

        // Add more batches to the storage.
        let mut storage = pool.connection().await.unwrap();
        let logs = gen_storage_logs(200..300, 5);
        extend_db_state(&mut storage, logs).await;

        // Wait until new tree versions have been checked.
        let health = wait_for_health(&health_check, |health| {
            if !matches!(health.status(), HealthStatus::Ready) {
                return false;
            }
            let details = health.details().unwrap();
            details.get("latest_checked_version") == Some(&10.into())
        })
        .await;
        let details = health.details().unwrap();
        assert_eq!(details["earliest_checked_version"], 6);
        assert_eq!(details["repaired_key_count"], 0);

        stop_sender.send_replace(true);
        calculator_handle.await.unwrap().unwrap();
        repair_task_handle.await.unwrap().unwrap();
    }
}
