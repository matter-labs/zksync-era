use std::{fmt, sync::Arc};

use tokio::sync::watch;
use zksync_block_reverter::BlockReverter;
use zksync_dal::{ConnectionPool, Core};
use zksync_health_check::AppHealthCheck;
use zksync_types::L1BatchNumber;

use crate::SnapshotRecoveryConfig;

/// A certain role of the node that can check basic properties of the node state,
/// e.g. whether it has to perform genesis, snapshot recovery, or do a rollback.
#[async_trait::async_trait]
pub trait NodeRole: fmt::Debug + Send + Sync + 'static {
    /// Performs genesis initialization if it's required.
    async fn genesis(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<()>;

    /// Checks whether genesis is already performed.
    async fn is_genesis_performed(&self, pool: &ConnectionPool<Core>) -> anyhow::Result<bool>;

    /// Performs (or continues) snapshot recovery.
    async fn snapshot_recovery(
        &self,
        stop_receiver: watch::Receiver<bool>,
        pool: &ConnectionPool<Core>,
        recovery_config: SnapshotRecoveryConfig,
        app_health: Arc<AppHealthCheck>,
    ) -> anyhow::Result<()>;

    /// Checks whether snapshot recovery is completed.
    async fn is_snapshot_recovery_completed(
        &self,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<bool>;

    /// Checks whether the node is in the state where it can perform a rollback.
    async fn should_rollback_to(
        &self,
        stop_receiver: watch::Receiver<bool>,
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<Option<L1BatchNumber>>;

    /// Performs a rollback to the specified batch.
    async fn perform_rollback(
        &self,
        reverter: BlockReverter,
        to_batch: L1BatchNumber,
    ) -> anyhow::Result<()>;
}
