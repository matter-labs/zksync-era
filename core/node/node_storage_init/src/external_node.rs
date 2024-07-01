use std::{sync::Arc, time::Instant};

use anyhow::{Context as _, Ok};
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_health_check::AppHealthCheck;
use zksync_object_store::ObjectStoreFactory;
use zksync_shared_metrics::{SnapshotRecoveryStage, APP_METRICS};
use zksync_snapshots_applier::{
    RecoveryCompletionStatus, SnapshotsApplierConfig, SnapshotsApplierTask,
};
use zksync_types::L2ChainId;
use zksync_web3_decl::client::{DynClient, L2};

use crate::SnapshotRecoveryConfig;

#[derive(Debug)]
pub struct ExternalNodeRole {
    pub l2_chain_id: L2ChainId,
    pub client: Box<DynClient<L2>>,
}

impl ExternalNodeRole {
    /// Will perform genesis initialization if it's required.
    /// If genesis is already performed, this method will do nothing.
    pub(crate) async fn genesis(self, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let mut storage = pool.connection_tagged("en").await?;
        zksync_node_sync::genesis::perform_genesis_if_needed(
            &mut storage,
            self.l2_chain_id,
            &self.client.for_component("genesis"),
        )
        .await
        .context("performing genesis failed")
    }

    pub(crate) async fn is_genesis_performed(
        &self,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<bool> {
        let mut storage = pool.connection_tagged("en").await?;
        let needed = zksync_node_sync::genesis::is_genesis_needed(&mut storage).await?;
        Ok(!needed)
    }

    pub(crate) async fn snapshot_recovery(
        self,
        stop_receiver: watch::Receiver<bool>,
        pool: ConnectionPool<Core>,
        recovery_config: SnapshotRecoveryConfig,
        app_health: Arc<AppHealthCheck>,
    ) -> anyhow::Result<()> {
        tracing::warn!("Proceeding with snapshot recovery. This is an experimental feature; use at your own risk");
        let object_store_config = recovery_config
            .object_store_config
            .context("Snapshot object store must be presented if snapshot recovery is activated")?;
        let object_store = ObjectStoreFactory::new(object_store_config)
            .create_store()
            .await?;

        let config = SnapshotsApplierConfig::default();
        let mut snapshots_applier_task = SnapshotsApplierTask::new(
            config,
            pool,
            Box::new(self.client.for_component("snapshot_recovery")),
            object_store,
        );
        if let Some(snapshot_l1_batch) = recovery_config.snapshot_l1_batch_override {
            tracing::info!(
                "Using a specific snapshot with L1 batch #{snapshot_l1_batch}; this may not work \
                     if the snapshot is too old (order of several weeks old) or non-existent"
            );
            snapshots_applier_task.set_snapshot_l1_batch(snapshot_l1_batch);
        }
        if recovery_config.drop_storage_key_preimages {
            tracing::info!("Dropping storage key preimages for snapshot storage logs");
            snapshots_applier_task.drop_storage_key_preimages();
        }
        app_health.insert_component(snapshots_applier_task.health_check())?;

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

    pub async fn is_snapshot_recovery_completed(
        &self,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<bool> {
        let mut storage = pool.connection_tagged("en").await?;
        let completed = matches!(
            SnapshotsApplierTask::is_recovery_completed(&mut storage, &self.client).await?,
            RecoveryCompletionStatus::Completed
        );
        Ok(completed)
    }
}
