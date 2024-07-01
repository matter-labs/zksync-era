use std::sync::Arc;

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_block_reverter::BlockReverter;
use zksync_config::ObjectStoreConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_health_check::AppHealthCheck;
use zksync_types::L1BatchNumber;

use crate::node_role::NodeRole;

pub use crate::{external_node::ExternalNodeRole, main_node::MainNodeRole};

mod external_node;
mod main_node;
mod node_role;

#[derive(Debug)]
pub struct SnapshotRecoveryConfig {
    /// If not specified, the latest snapshot will be used.
    pub snapshot_l1_batch_override: Option<L1BatchNumber>,
    pub drop_storage_key_preimages: bool,
    pub object_store_config: Option<ObjectStoreConfig>,
}

#[derive(Debug)]
enum InitDecision {
    /// Perform or check genesis.
    Genesis,
    /// Perform or check snapshot recovery.
    SnapshotRecovery,
}

#[derive(Debug)]
pub struct NodeStorageInitializer {
    pool: ConnectionPool<Core>,
    node_role: NodeRole,
    app_health: Arc<AppHealthCheck>,
    recovery_config: Option<SnapshotRecoveryConfig>,
    block_reverter: Option<BlockReverter>,
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
            app_health,
            recovery_config: None,
            block_reverter: None,
        }
    }

    pub fn with_recovery_config(mut self, recovery_config: Option<SnapshotRecoveryConfig>) -> Self {
        self.recovery_config = recovery_config;
        self
    }

    pub fn with_block_reverter(mut self, block_reverter: Option<BlockReverter>) -> Self {
        self.block_reverter = block_reverter;
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

    /// Checks if the node can safely start operating.
    pub async fn storage_initialized(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<bool> {
        let decision = self.decision().await?;
        let mut initialized = match decision {
            InitDecision::Genesis => self.node_role.is_genesis_performed(&self.pool).await?,
            InitDecision::SnapshotRecovery => {
                self.node_role
                    .is_snapshot_recovery_completed(&self.pool)
                    .await?
            }
        };
        if !initialized {
            return Ok(false);
        }

        if self
            .node_role
            .should_rollback_to(stop_receiver, &self.pool)
            .await?
            .is_some()
        {
            // Node is in the incorrect state. Not safe to proceed.
            initialized = false;
        }

        Ok(initialized)
    }

    /// Initializes the storage for the node.
    /// After the initialization, the node can safely start operating.
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let decision = self.decision().await?;

        // Make sure that we have state to work with.
        match decision {
            InitDecision::Genesis => {
                tracing::info!("Performing genesis initialization");
                self.node_role.genesis(&self.pool).await?;
            }
            InitDecision::SnapshotRecovery => {
                tracing::info!("Performing snapshot recovery initialization");
                let recovery_config = self.recovery_config.context(
                    "Snapshot recovery is required to proceed, but it is not enabled. \
                    Consult the documentation for more information on how to enable it.",
                )?;
                self.node_role
                    .snapshot_recovery(
                        stop_receiver.clone(),
                        &self.pool,
                        recovery_config,
                        self.app_health.clone(),
                    )
                    .await?;
            }
        }

        // Now we may check whether we're in the invalid state and should perform a rollback.
        if let Some(to_batch) = self
            .node_role
            .should_rollback_to(stop_receiver, &self.pool)
            .await?
        {
            tracing::info!(l1_batch = %to_batch, "State must be rolled back to L1 batch");
            let Some(reverter) = self.block_reverter else {
                anyhow::bail!("Blocks must be reverted, but the block reverter is not provided.");
            };

            tracing::info!("Performing the rollback");
            self.node_role.perform_rollback(reverter, to_batch).await?;
        }

        Ok(())
    }
}
