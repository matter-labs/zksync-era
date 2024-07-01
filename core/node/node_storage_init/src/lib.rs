use std::sync::Arc;

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_config::ObjectStoreConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_health_check::AppHealthCheck;
use zksync_types::L1BatchNumber;

pub use self::{external_node::ExternalNodeRole, main_node::MainNodeRole};

mod external_node;
mod main_node;

#[derive(Debug)]
pub struct SnapshotRecoveryConfig {
    /// If not specified, the latest snapshot will be used.
    pub snapshot_l1_batch_override: Option<L1BatchNumber>,
    pub drop_storage_key_preimages: bool,
    pub object_store_config: Option<ObjectStoreConfig>,
}

#[derive(Debug)]
pub enum InitDecision {
    /// Perform or check genesis.
    Genesis,
    /// Perform or check snapshot recovery.
    SnapshotRecovery,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum NodeRole {
    /// Storage must be initialized for the main node.
    Main(MainNodeRole),
    /// Storage must be initialized for the external node.
    External(ExternalNodeRole),
}

impl From<MainNodeRole> for NodeRole {
    fn from(main: MainNodeRole) -> Self {
        Self::Main(main)
    }
}

impl From<ExternalNodeRole> for NodeRole {
    fn from(en: ExternalNodeRole) -> Self {
        Self::External(en)
    }
}

#[derive(Debug)]
pub struct NodeStorageInitializer {
    pool: ConnectionPool<Core>,
    node_role: NodeRole,
    recovery_config: Option<SnapshotRecoveryConfig>,
    app_health: Arc<AppHealthCheck>,
}

impl NodeStorageInitializer {
    pub fn new(
        role: impl Into<NodeRole>,
        pool: ConnectionPool<Core>,
        app_health: Arc<AppHealthCheck>,
    ) -> Self {
        Self {
            pool,
            node_role: role.into(),
            recovery_config: None,
            app_health,
        }
    }

    pub fn with_recovery_config(mut self, recovery_config: Option<SnapshotRecoveryConfig>) -> Self {
        self.recovery_config = recovery_config;
        self
    }

    /// Returns the preferred kind of storage initialization.
    /// The decision is based on the current state of the storage.
    /// Note that the decision does not guarantee that the initialization has not been performed
    /// already, so any returned decision should be checked before performing the initialization.
    async fn decision(&self) -> anyhow::Result<InitDecision> {
        let mut storage = self.pool.connection().await?;
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
                tracing::info!(
                    "Node has a genesis L1 batch: {batch:?} and no snapshot recovery info"
                );
                InitDecision::Genesis
            }
            (None, Some(snapshot_recovery)) => {
                tracing::info!("Node has no genesis L1 batch and snapshot recovery information: {snapshot_recovery:?}");
                // We don't know whether the recovery is completed or still in progress.
                InitDecision::SnapshotRecovery
            }
            (None, None) => {
                tracing::info!("Node has neither genesis L1 batch, nor snapshot recovery info");
                if self.recovery_config.is_some() {
                    InitDecision::SnapshotRecovery
                } else {
                    InitDecision::Genesis
                }
            }
        };
        Ok(decision)
    }

    pub async fn storage_initialized(&self) -> anyhow::Result<bool> {
        let decision = self.decision().await?;
        let initialized = match (&self.node_role, decision) {
            (NodeRole::Main(main), InitDecision::Genesis) => {
                main.is_genesis_performed(&self.pool).await?
            }
            (NodeRole::Main(_), InitDecision::SnapshotRecovery) => {
                anyhow::bail!("Snapshot recovery is not supported for the main node")
            }
            (NodeRole::External(en), InitDecision::Genesis) => {
                en.is_genesis_performed(&self.pool).await?
            }
            (NodeRole::External(en), InitDecision::SnapshotRecovery) => {
                en.is_snapshot_recovery_completed(&self.pool).await?
            }
        };

        Ok(initialized)
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let decision = self.decision().await?;

        match (self.node_role, decision) {
            (NodeRole::Main(main), InitDecision::Genesis) => main.genesis(&self.pool).await?,
            (NodeRole::Main(_), InitDecision::SnapshotRecovery) => {
                anyhow::bail!("Snapshot recovery is not supported for the main node")
            }
            (NodeRole::External(en), InitDecision::Genesis) => en.genesis(&self.pool).await?,
            (NodeRole::External(en), InitDecision::SnapshotRecovery) => {
                let recovery_config = self.recovery_config.context(
                    "Snapshot recovery is required to proceed, but it is not enabled. Enable by setting \
                     `EN_SNAPSHOTS_RECOVERY_ENABLED=true` env variable to the node binary, or use a Postgres dump for recovery"
                )?;
                en.snapshot_recovery(stop_receiver, self.pool, recovery_config, self.app_health)
                    .await?;
            }
        }

        Ok(())
    }
}
