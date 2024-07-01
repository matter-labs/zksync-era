use std::sync::Arc;

use tokio::sync::watch;
use zksync_block_reverter::BlockReverter;
use zksync_dal::{ConnectionPool, Core};
use zksync_health_check::AppHealthCheck;
use zksync_types::L1BatchNumber;

use crate::{ExternalNodeRole, MainNodeRole, SnapshotRecoveryConfig};

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

impl NodeRole {
    pub(crate) async fn genesis(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<()> {
        match self {
            Self::Main(main) => main.genesis(pool).await,
            Self::External(en) => en.genesis(pool).await,
        }
    }

    pub(crate) async fn is_genesis_performed(
        &self,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<bool> {
        match self {
            Self::Main(main) => main.is_genesis_performed(pool).await,
            Self::External(en) => en.is_genesis_performed(pool).await,
        }
    }

    pub(crate) async fn snapshot_recovery(
        &self,
        stop_receiver: watch::Receiver<bool>,
        pool: &ConnectionPool<Core>,
        recovery_config: SnapshotRecoveryConfig,
        app_health: Arc<AppHealthCheck>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Main(main) => {
                main.snapshot_recovery(stop_receiver, pool, recovery_config, app_health)
                    .await
            }
            Self::External(en) => {
                en.snapshot_recovery(stop_receiver, pool, recovery_config, app_health)
                    .await
            }
        }
    }

    pub(crate) async fn is_snapshot_recovery_completed(
        &self,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<bool> {
        match self {
            Self::Main(main) => main.is_snapshot_recovery_completed(pool).await,
            Self::External(en) => en.is_snapshot_recovery_completed(pool).await,
        }
    }

    pub(crate) async fn should_rollback_to(
        &self,
        stop_receiver: watch::Receiver<bool>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<Option<L1BatchNumber>> {
        match self {
            Self::Main(main) => main.should_rollback_to(stop_receiver, pool).await,
            Self::External(en) => en.should_rollback_to(stop_receiver, pool).await,
        }
    }

    pub(crate) async fn perform_rollback(
        &self,
        reverter: BlockReverter,
        to_batch: L1BatchNumber,
    ) -> anyhow::Result<()> {
        match self {
            Self::Main(main) => main.perform_rollback(reverter, to_batch).await,
            Self::External(en) => en.perform_rollback(reverter, to_batch).await,
        }
    }
}
