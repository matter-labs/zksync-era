//! EN initialization logic.

use std::time::Instant;

use anyhow::Context as _;
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_health_check::AppHealthCheck;
use zksync_node_sync::genesis::perform_genesis_if_needed;
use zksync_object_store::ObjectStoreFactory;
use zksync_shared_metrics::{SnapshotRecoveryStage, APP_METRICS};
use zksync_snapshots_applier::{SnapshotsApplierConfig, SnapshotsApplierTask};
use zksync_types::{L1BatchNumber, L2ChainId};
use zksync_web3_decl::client::{DynClient, L2};

use crate::config::snapshot_recovery_object_store_config;

#[derive(Debug)]
pub(crate) struct SnapshotRecoveryConfig {
    /// If not specified, the latest snapshot will be used.
    pub snapshot_l1_batch_override: Option<L1BatchNumber>,
}

#[derive(Debug)]
enum InitDecision {
    /// Perform or check genesis.
    Genesis,
    /// Perform or check snapshot recovery.
    SnapshotRecovery,
}

pub(crate) async fn ensure_storage_initialized(
    pool: ConnectionPool<Core>,
    main_node_client: Box<DynClient<L2>>,
    app_health: &AppHealthCheck,
    l2_chain_id: L2ChainId,
    recovery_config: Option<SnapshotRecoveryConfig>,
) -> anyhow::Result<()> {
    let mut storage = pool.connection_tagged("en").await?;
    let genesis_l1_batch = storage
        .blocks_dal()
        .get_l1_batch_header(L1BatchNumber(0))
        .await?;
    let snapshot_recovery = storage
        .snapshot_recovery_dal()
        .get_applied_snapshot_status()
        .await?;
    drop(storage);

    let decision = match (genesis_l1_batch, snapshot_recovery) {
        (Some(batch), Some(snapshot_recovery)) => {
            anyhow::bail!(
                "Node has both genesis L1 batch: {batch:?} and snapshot recovery information: {snapshot_recovery:?}. \
                 This is not supported and can be caused by broken snapshot recovery."
            );
        }
        (Some(batch), None) => {
            tracing::info!("Node has a genesis L1 batch: {batch:?} and no snapshot recovery info");
            InitDecision::Genesis
        }
        (None, Some(snapshot_recovery)) => {
            tracing::info!("Node has no genesis L1 batch and snapshot recovery information: {snapshot_recovery:?}");
            InitDecision::SnapshotRecovery
        }
        (None, None) => {
            tracing::info!("Node has neither genesis L1 batch, nor snapshot recovery info");
            if recovery_config.is_some() {
                InitDecision::SnapshotRecovery
            } else {
                InitDecision::Genesis
            }
        }
    };

    tracing::info!("Chosen node initialization strategy: {decision:?}");
    match decision {
        InitDecision::Genesis => {
            let mut storage = pool.connection_tagged("en").await?;
            perform_genesis_if_needed(
                &mut storage,
                l2_chain_id,
                &main_node_client.for_component("genesis"),
            )
            .await
            .context("performing genesis failed")?;
        }
        InitDecision::SnapshotRecovery => {
            let recovery_config = recovery_config.context(
                "Snapshot recovery is required to proceed, but it is not enabled. Enable by setting \
                 `EN_SNAPSHOTS_RECOVERY_ENABLED=true` env variable to the node binary, or use a Postgres dump for recovery"
            )?;

            tracing::warn!("Proceeding with snapshot recovery. This is an experimental feature; use at your own risk");
            let object_store_config = snapshot_recovery_object_store_config()?;
            let object_store = ObjectStoreFactory::new(object_store_config)
                .create_store()
                .await?;

            let config = SnapshotsApplierConfig::default();
            let mut snapshots_applier_task = SnapshotsApplierTask::new(
                config,
                pool,
                Box::new(main_node_client.for_component("snapshot_recovery")),
                object_store,
            );
            if let Some(snapshot_l1_batch) = recovery_config.snapshot_l1_batch_override {
                tracing::info!(
                    "Using a specific snapshot with L1 batch #{snapshot_l1_batch}; this may not work \
                     if the snapshot is too old (order of several weeks old) or non-existent"
                );
                snapshots_applier_task.set_snapshot_l1_batch(snapshot_l1_batch);
            }
            app_health.insert_component(snapshots_applier_task.health_check())?;

            let recovery_started_at = Instant::now();
            let stats = snapshots_applier_task
                .run()
                .await
                .context("snapshot recovery failed")?;
            if stats.done_work {
                let latency = recovery_started_at.elapsed();
                APP_METRICS.snapshot_recovery_latency[&SnapshotRecoveryStage::Postgres]
                    .set(latency);
                tracing::info!("Recovered Postgres from snapshot in {latency:?}");
            }
        }
    }
    Ok(())
}
