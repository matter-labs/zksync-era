use std::{num::NonZeroUsize, sync::Arc, time::Instant};

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_health_check::AppHealthCheck;
use zksync_object_store::ObjectStoreFactory;
use zksync_shared_metrics::{SnapshotRecoveryStage, APP_METRICS};
use zksync_snapshots_applier::{
    RecoveryCompletionStatus, SnapshotsApplierConfig, SnapshotsApplierTask,
};
use zksync_types::OrStopped;
use zksync_web3_decl::client::{DynClient, L2};

use crate::{InitializeStorage, SnapshotRecoveryConfig};

#[derive(Debug)]
pub struct ExternalNodeSnapshotRecovery {
    pub client: Box<DynClient<L2>>,
    pub pool: ConnectionPool<Core>,
    pub max_concurrency: NonZeroUsize,
    pub recovery_config: SnapshotRecoveryConfig,
    pub app_health: Arc<AppHealthCheck>,
}

#[async_trait::async_trait]
impl InitializeStorage for ExternalNodeSnapshotRecovery {
    async fn initialize_storage(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> Result<(), OrStopped> {
        tracing::warn!("Proceeding with snapshot recovery. This is an experimental feature; use at your own risk");

        let pool_size = self.pool.max_size() as usize;
        if pool_size < self.max_concurrency.get() + 1 {
            tracing::error!(
                "Connection pool has insufficient number of connections ({pool_size} vs concurrency {} + 1 connection for checks). \
                 This will likely lead to pool starvation during recovery.",
                self.max_concurrency
            );
        }

        let object_store_config =
            self.recovery_config.object_store_config.clone().context(
                "Snapshot object store must be presented if snapshot recovery is activated",
            )?;
        let object_store = ObjectStoreFactory::new(object_store_config)
            .create_store()
            .await?;

        let config = SnapshotsApplierConfig {
            max_concurrency: self.max_concurrency,
            ..SnapshotsApplierConfig::default()
        };
        let mut snapshots_applier_task = SnapshotsApplierTask::new(
            config,
            self.pool.clone(),
            Box::new(self.client.clone().for_component("snapshot_recovery")),
            object_store,
        );
        if let Some(snapshot_l1_batch) = self.recovery_config.snapshot_l1_batch_override {
            tracing::info!(
                "Using a specific snapshot with L1 batch #{snapshot_l1_batch}; this may not work \
                     if the snapshot is too old (order of several weeks old) or non-existent"
            );
            snapshots_applier_task.set_snapshot_l1_batch(snapshot_l1_batch);
        }
        if self.recovery_config.drop_storage_key_preimages {
            tracing::info!("Dropping storage key preimages for snapshot storage logs");
            snapshots_applier_task.drop_storage_key_preimages();
        }
        self.app_health
            .insert_component(snapshots_applier_task.health_check())
            .map_err(OrStopped::internal)?;

        let recovery_started_at = Instant::now();
        let stats = snapshots_applier_task.run(stop_receiver).await?;
        if stats.done_work {
            let latency = recovery_started_at.elapsed();
            APP_METRICS.snapshot_recovery_latency[&SnapshotRecoveryStage::Postgres].set(latency);
            tracing::info!("Recovered Postgres from snapshot in {latency:?}");
        }
        // We don't really care if the task was canceled.
        // If it was, all the other tasks are canceled as well.

        Ok(())
    }

    async fn is_initialized(&self) -> anyhow::Result<bool> {
        let mut storage = self.pool.connection_tagged("en").await?;
        let completed = matches!(
            SnapshotsApplierTask::is_recovery_completed(&mut storage, &self.client).await?,
            RecoveryCompletionStatus::Completed
        );
        Ok(completed)
    }
}

#[cfg(test)]
mod tests {
    use std::future;

    use zksync_types::{
        tokens::{TokenInfo, TokenMetadata},
        Address, L2BlockNumber,
    };
    use zksync_web3_decl::client::MockClient;

    use super::*;

    #[tokio::test]
    async fn recovery_does_not_starve_pool_connections() {
        let pool = ConnectionPool::constrained_test_pool(5).await;
        let app_health = Arc::new(AppHealthCheck::new(None, None));
        let client = MockClient::builder(L2::default())
            .method("en_syncTokens", |_number: Option<L2BlockNumber>| {
                Ok(vec![TokenInfo {
                    l1_address: Address::repeat_byte(1),
                    l2_address: Address::repeat_byte(2),
                    metadata: TokenMetadata {
                        name: "test".to_string(),
                        symbol: "TEST".to_string(),
                        decimals: 18,
                    },
                }])
            })
            .build();
        let recovery = ExternalNodeSnapshotRecovery {
            client: Box::new(client),
            pool,
            max_concurrency: NonZeroUsize::new(4).unwrap(),
            recovery_config: SnapshotRecoveryConfig {
                snapshot_l1_batch_override: None,
                drop_storage_key_preimages: false,
                object_store_config: None,
            },
            app_health,
        };

        // Emulate recovery by indefinitely holding onto `max_concurrency` connections. In practice,
        // the snapshot applier will release connections eventually, but it may require more time than the connection
        // acquisition timeout configured for the DB pool.
        for _ in 0..recovery.max_concurrency.get() {
            let connection = recovery.pool.connection().await.unwrap();
            tokio::spawn(async move {
                future::pending::<()>().await;
                drop(connection);
            });
        }

        // The only token reported by the mock client isn't recovered
        assert!(!recovery.is_initialized().await.unwrap());
    }
}
