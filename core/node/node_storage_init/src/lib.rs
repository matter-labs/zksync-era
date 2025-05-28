use std::{future::Future, sync::Arc, time::Duration};

use tokio::sync::watch;
use zksync_config::ObjectStoreConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_types::{try_stoppable, L1BatchNumber, OrStopped, StopContext};

pub use crate::traits::{InitializeStorage, RevertStorage};

pub mod external_node;
pub mod main_node;
pub mod node;
mod traits;

#[derive(Debug)]
pub struct SnapshotRecoveryConfig {
    /// If not specified, the latest snapshot will be used.
    pub snapshot_l1_batch_override: Option<L1BatchNumber>,
    pub drop_storage_key_preimages: bool,
    pub object_store_config: Option<ObjectStoreConfig>,
}

#[derive(Debug, Clone, Copy)]
enum InitDecision {
    /// Perform or check genesis.
    Genesis,
    /// Perform or check snapshot recovery.
    SnapshotRecovery,
}

#[derive(Debug, Clone)]
pub struct NodeInitializationStrategy {
    pub genesis: Arc<dyn InitializeStorage>,
    pub snapshot_recovery: Option<Arc<dyn InitializeStorage>>,
    pub block_reverter: Option<Arc<dyn RevertStorage>>,
}

/// Node storage initializer.
/// This structure is responsible for making sure that the node storage is initialized.
///
/// This structure operates together with [`NodeRole`] to achieve that:
/// `NodeStorageInitializer` understands what does initialized storage mean, but it defers
/// any actual initialization to the `NodeRole` implementation. This allows to have different
/// initialization strategies for different node types, while keeping common invariants
/// for the whole system.
#[derive(Debug)]
pub struct NodeStorageInitializer {
    strategy: NodeInitializationStrategy,
    pool: ConnectionPool<Core>,
}

impl NodeStorageInitializer {
    pub fn new(strategy: NodeInitializationStrategy, pool: ConnectionPool<Core>) -> Self {
        Self { strategy, pool }
    }

    /// Returns the preferred kind of storage initialization.
    /// The decision is based on the current state of the storage.
    /// Note that the decision does not guarantee that the initialization has not been performed
    /// already, so any returned decision should be checked before performing the initialization.
    async fn decision(&self) -> anyhow::Result<InitDecision> {
        let mut storage = self.pool.connection_tagged("node_init").await?;
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
                InitDecision::SnapshotRecovery
            }
            (None, None) => {
                tracing::info!("Node has neither genesis L1 batch, nor snapshot recovery info");
                if self.strategy.snapshot_recovery.is_some() {
                    InitDecision::SnapshotRecovery
                } else {
                    InitDecision::Genesis
                }
            }
        };
        Ok(decision)
    }

    /// Initializes the storage for the node.
    /// After the initialization, the node can safely start operating.
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let decision = self.decision().await?;

        // Make sure that we have state to work with.
        match decision {
            InitDecision::Genesis => {
                tracing::info!("Performing genesis initialization");
                try_stoppable!(
                    self.strategy
                        .genesis
                        .initialize_storage(stop_receiver.clone())
                        .await
                );
            }
            InitDecision::SnapshotRecovery => {
                tracing::info!("Performing snapshot recovery initialization");
                if let Some(recovery) = &self.strategy.snapshot_recovery {
                    try_stoppable!(recovery.initialize_storage(stop_receiver.clone()).await);
                } else {
                    anyhow::bail!(
                        "Snapshot recovery should be performed, but the strategy is not provided. \
                        In most of the cases this error means that the node was first started \
                        with snapshots recovery enabled, but then it was disabled. \
                        To get rid of this error and have the node sync from genesis \
                        please clear the Node's database"
                    );
                }
            }
        }

        // Now we may check whether we're in the invalid state and should perform a rollback.
        if let Some(reverter) = &self.strategy.block_reverter {
            if let Some(to_batch) =
                try_stoppable!(reverter.last_correct_batch_for_reorg(stop_receiver).await)
            {
                tracing::info!(l1_batch = %to_batch, "State must be rolled back to L1 batch");
                tracing::info!("Performing the rollback");
                reverter.revert_storage(to_batch).await?;
            }
        }

        Ok(())
    }

    /// Checks if the node can safely start operating.
    pub async fn wait_for_initialized_storage(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> Result<(), OrStopped> {
        const POLLING_INTERVAL: Duration = Duration::from_secs(1);

        // Wait until data is added to the database.
        poll(stop_receiver.clone(), POLLING_INTERVAL, || {
            self.is_database_initialized()
        })
        .await?;

        // Wait until the rollback is no longer needed.
        poll(stop_receiver.clone(), POLLING_INTERVAL, || {
            self.is_chain_tip_correct(stop_receiver.clone())
        })
        .await
    }

    async fn is_database_initialized(&self) -> anyhow::Result<bool> {
        // We're fine if the database is initialized in any meaningful way we can check.
        if self.strategy.genesis.is_initialized().await? {
            return Ok(true);
        }
        if let Some(snapshot_recovery) = &self.strategy.snapshot_recovery {
            return snapshot_recovery.is_initialized().await;
        }
        Ok(false)
    }

    /// Checks if the head of the chain has correct state, e.g. no rollback needed.
    async fn is_chain_tip_correct(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<bool> {
        // May be `true` if stop request is received, but the node will shut down without launching any tasks anyway.
        let initialized = if let Some(reverter) = &self.strategy.block_reverter {
            !reverter
                .is_reorg_needed(stop_receiver)
                .await
                .unwrap_stopped(false)?
        } else {
            true
        };
        Ok(initialized)
    }
}

async fn poll<F, Fut>(
    mut stop_receiver: watch::Receiver<bool>,
    polling_interval: Duration,
    mut check: F,
) -> Result<(), OrStopped>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = anyhow::Result<bool>>,
{
    while !*stop_receiver.borrow() && !check().await? {
        // Return value will be checked on the next iteration.
        tokio::time::timeout(polling_interval, stop_receiver.changed())
            .await
            .ok();
    }

    if *stop_receiver.borrow() {
        Err(OrStopped::Stopped)
    } else {
        Ok(())
    }
}
