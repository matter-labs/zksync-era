use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_health_check::AppHealthCheck;
use zksync_object_store::ObjectStoreFactory;
use zksync_shared_metrics::{SnapshotRecoveryStage, APP_METRICS};
use zksync_snapshots_applier::{
    RecoveryCompletionStatus, SnapshotsApplierConfig, SnapshotsApplierTask,
};
use zksync_web3_decl::client::{DynClient, L2};

use crate::{InitializeStorage, SnapshotRecoveryConfig};

#[derive(Debug)]
pub struct ExternalNodeSnapshotRecovery {
    pub client: Box<DynClient<L2>>,
    pub pool: ConnectionPool<Core>,
    pub recovery_config: SnapshotRecoveryConfig,
    pub app_health: Arc<AppHealthCheck>,
}

#[async_trait::async_trait]
impl InitializeStorage for ExternalNodeSnapshotRecovery {
    async fn initialize_storage(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        tracing::warn!("Proceeding with snapshot recovery. This is an experimental feature; use at your own risk");
        let object_store_config =
            self.recovery_config.object_store_config.clone().context(
                "Snapshot object store must be presented if snapshot recovery is activated",
            )?;
        let object_store = ObjectStoreFactory::new(object_store_config)
            .create_store()
            .await?;

        let config = SnapshotsApplierConfig::default();
        let mut snapshots_applier_task = SnapshotsApplierTask::new(
            config,
            pool,
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
            .insert_component(snapshots_applier_task.health_check())?;

        let recovery_started_at = Instant::now();
        let stats = snapshots_applier_task
            .run(stop_receiver)
            .await
            .context("snapshot recovery failed")?;
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
