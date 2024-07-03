use std::{future::Future, sync::Arc, time::Duration};

use tokio::sync::watch;
use zksync_config::ObjectStoreConfig;
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_types::L1BatchNumber;

pub use crate::traits::{InitializeStorage, RevertStorage};

pub mod external_node;
pub mod main_node;
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

#[derive(Debug)]
pub struct NodeInitializationStrategy {
    pub genesis: Box<dyn InitializeStorage>,
    pub snapshot_recovery: Option<Box<dyn InitializeStorage>>,
    pub block_reverter: Option<Box<dyn RevertStorage>>,
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
    strategy: Arc<NodeInitializationStrategy>,
    pool: ConnectionPool<Core>,
}

impl NodeStorageInitializer {
    pub fn new(strategy: Arc<NodeInitializationStrategy>, pool: ConnectionPool<Core>) -> Self {
        Self { strategy, pool }
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
                self.strategy
                    .genesis
                    .initialize_storage(stop_receiver.clone())
                    .await?;
            }
            InitDecision::SnapshotRecovery => {
                tracing::info!("Performing snapshot recovery initialization");
                if let Some(recovery) = &self.strategy.snapshot_recovery {
                    recovery.initialize_storage(stop_receiver.clone()).await?;
                } else {
                    anyhow::bail!(
                        "Snapshot recovery should be performed, but the strategy is not provided"
                    );
                }
            }
        }

        // Now we may check whether we're in the invalid state and should perform a rollback.
        if let Some(reverter) = &self.strategy.block_reverter {
            if let Some(to_batch) = reverter.is_revert_needed().await? {
                tracing::info!(l1_batch = %to_batch, "State must be rolled back to L1 batch");
                tracing::info!("Performing the rollback");
                reverter.revert_storage(to_batch, stop_receiver).await?;
            }
        }

        Ok(())
    }

    /// Checks if the node can safely start operating.
    pub async fn wait_for_initialized_storage(
        &self,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        const POLLING_INTERVAL: Duration = Duration::from_secs(1);

        let decision = self.decision().await?;

        // Wait until data is added to the database.
        poll(stop_receiver.clone(), POLLING_INTERVAL, || {
            self.is_database_initialized(decision)
        })
        .await?;
        if *stop_receiver.borrow() {
            return Ok(());
        }

        // Wait until the rollback is no longer needed.
        poll(stop_receiver.clone(), POLLING_INTERVAL, || {
            self.is_rollback_not_needed(stop_receiver.clone())
        })
        .await?;

        Ok(())
    }

    async fn is_database_initialized(&self, decision: InitDecision) -> anyhow::Result<bool> {
        match decision {
            InitDecision::Genesis => self.strategy.genesis.is_initialized().await,
            InitDecision::SnapshotRecovery => {
                if let Some(recovery) = &self.strategy.snapshot_recovery {
                    recovery.is_initialized().await
                } else {
                    anyhow::bail!(
                        "Snapshot recovery should be performed, but the strategy is not provided"
                    );
                }
            }
        }
    }

    async fn is_rollback_not_needed(
        &self,
        _stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<bool> {
        let initialized = if let Some(reverter) = &self.strategy.block_reverter {
            reverter.is_revert_needed().await?.is_none()
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
) -> anyhow::Result<()>
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

    Ok(())
}
