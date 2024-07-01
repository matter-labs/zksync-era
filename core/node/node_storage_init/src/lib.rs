use tokio::sync::watch;
use zksync_config::{ContractsConfig, GenesisConfig, ObjectStoreConfig};
use zksync_dal::{ConnectionPool, Core, CoreDal as _};
use zksync_types::L1BatchNumber;
use zksync_web3_decl::client::{DynClient, L1, L2};

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
    /// The storage is already initialized.
    Skip,
    /// Perform or check genesis.
    Genesis,
    /// Perform or check snapshot recovery.
    SnapshotRecovery,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum NodeRole {
    Main(MainNodeRole),
    External(ExternalNodeRole),
}

#[derive(Debug)]
pub struct NodeStorageInitializer {
    pool: ConnectionPool<Core>,
    node_role: NodeRole,
    recovery_config: Option<SnapshotRecoveryConfig>,
}

impl NodeStorageInitializer {
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
                InitDecision::Skip
            }
            (None, Some(snapshot_recovery)) => {
                tracing::info!("Node has no genesis L1 batch and snapshot recovery information: {snapshot_recovery:?}");
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

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        Ok(())
    }
}
