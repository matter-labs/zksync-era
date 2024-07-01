use std::{sync::Arc, time::Instant};

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_block_reverter::BlockReverter;
use zksync_dal::{ConnectionPool, Core};
use zksync_health_check::AppHealthCheck;
use zksync_object_store::ObjectStoreFactory;
use zksync_reorg_detector::ReorgDetector;
use zksync_shared_metrics::{SnapshotRecoveryStage, APP_METRICS};
use zksync_snapshots_applier::{
    RecoveryCompletionStatus, SnapshotsApplierConfig, SnapshotsApplierTask,
};
use zksync_types::{L1BatchNumber, L2ChainId};
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
    pub(crate) async fn genesis(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        let mut storage = pool.connection_tagged("en").await?;
        zksync_node_sync::genesis::perform_genesis_if_needed(
            &mut storage,
            self.l2_chain_id,
            &self.client.clone().for_component("genesis"),
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
        &self,
        stop_receiver: watch::Receiver<bool>,
        pool: &ConnectionPool<Core>,
        recovery_config: SnapshotRecoveryConfig,
        app_health: Arc<AppHealthCheck>,
    ) -> anyhow::Result<()> {
        let pool = pool.clone();
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
            Box::new(self.client.clone().for_component("snapshot_recovery")),
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

    pub(crate) async fn is_snapshot_recovery_completed(
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

    pub(crate) async fn should_rollback_to(
        &self,
        stop_receiver: watch::Receiver<bool>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<Option<L1BatchNumber>> {
        let mut reorg_detector = ReorgDetector::new(self.client.clone(), pool.clone());
        let batch = match reorg_detector.run_once(stop_receiver).await {
            Ok(_) => {
                // Even if stop signal was received, the node will shut down without launching any tasks.
                tracing::info!("No rollback was detected");
                None
            }
            Err(zksync_reorg_detector::Error::ReorgDetected(last_correct_l1_batch)) => {
                tracing::info!("Reverting to l1 batch number {last_correct_l1_batch}");
                Some(last_correct_l1_batch)
            }
            Err(err) => return Err(err).context("reorg_detector.check_consistency()"),
        };
        Ok(batch)
    }

    pub(crate) async fn perform_rollback(
        &self,
        reverter: BlockReverter,
        to_batch: L1BatchNumber,
    ) -> anyhow::Result<()> {
        tracing::info!("Reverting to l1 batch number {to_batch}");
        reverter.roll_back(to_batch).await?;
        tracing::info!("Revert successfully completed");
        Ok(())
    }
}
