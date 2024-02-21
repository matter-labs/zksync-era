//! EN initialization logic.

use anyhow::Context as _;
use zksync_basic_types::{L1BatchNumber, L2ChainId};
use zksync_core::sync_layer::genesis::perform_genesis_if_needed;
use zksync_dal::ConnectionPool;
use zksync_object_store::ObjectStoreFactory;
use zksync_snapshots_applier::{SnapshotsApplierConfig, SnapshotsApplierOutcome};
use zksync_web3_decl::jsonrpsee::http_client::HttpClient;

use crate::config::read_snapshots_recovery_config;

#[derive(Debug)]
enum InitDecision {
    /// Perform or check genesis.
    Genesis,
    /// Perform or check snapshot recovery.
    SnapshotRecovery,
}

pub(crate) async fn ensure_storage_initialized(
    pool: &ConnectionPool,
    main_node_client: &HttpClient,
    l2_chain_id: L2ChainId,
    consider_snapshot_recovery: bool,
) -> anyhow::Result<()> {
    let mut storage = pool.access_storage_tagged("en").await?;
    let genesis_l1_batch = storage
        .blocks_dal()
        .get_l1_batch_header(L1BatchNumber(0))
        .await
        .context("failed getting genesis batch info")?;
    let snapshot_recovery = storage
        .snapshot_recovery_dal()
        .get_applied_snapshot_status()
        .await
        .context("failed getting snapshot recovery info")?;
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
            if consider_snapshot_recovery {
                InitDecision::SnapshotRecovery
            } else {
                InitDecision::Genesis
            }
        }
    };

    tracing::info!("Chosen node initialization strategy: {decision:?}");
    match decision {
        InitDecision::Genesis => {
            let mut storage = pool.access_storage_tagged("en").await?;
            perform_genesis_if_needed(&mut storage, l2_chain_id, main_node_client)
                .await
                .context("performing genesis failed")?;
        }
        InitDecision::SnapshotRecovery => {
            anyhow::ensure!(
                consider_snapshot_recovery,
                "Snapshot recovery is required to proceed, but it is not enabled. Enable by supplying \
                 `--enable-snapshots-recovery` command-line arg to the node binary, or reset the node storage \
                 to sync from genesis"
            );

            tracing::warn!("Proceeding with snapshot recovery. This is an experimental feature; use at your own risk");
            let recovery_config = read_snapshots_recovery_config()?;
            let blob_store = ObjectStoreFactory::new(recovery_config.snapshots_object_store)
                .create_store()
                .await;
            let outcome = SnapshotsApplierConfig::default()
                .run(pool, main_node_client, &blob_store)
                .await
                .context("snapshot recovery failed")?;
            match outcome {
                SnapshotsApplierOutcome::Ok => {
                    tracing::info!("Snapshot recovery is complete");
                }
                SnapshotsApplierOutcome::NoSnapshotsOnMainNode => {
                    anyhow::bail!("No snapshots on main node; snapshot recovery is impossible");
                }
                SnapshotsApplierOutcome::InitializedWithoutSnapshot => {
                    anyhow::bail!(
                        "Node contains a non-genesis L1 batch, but no genesis; snapshot recovery is unsafe. \
                         This should never occur unless the node DB was manually tampered with"
                    );
                }
            }
        }
    }
    Ok(())
}
