//! High-level recovery logic for the Merkle tree.

use anyhow::Context as _;
use tokio::sync::watch;

use zksync_dal::ConnectionPool;
use zksync_health_check::HealthUpdater;
use zksync_types::L1BatchNumber;

use super::helpers::{AsyncTree, AsyncTreeRecovery, GenericAsyncTree};

impl GenericAsyncTree {
    pub async fn ensure_ready(
        self,
        pool: &ConnectionPool,
        _stop_receiver: &watch::Receiver<bool>,
        _health_updater: &HealthUpdater,
    ) -> anyhow::Result<AsyncTree> {
        let _tree = match self {
            Self::Ready(tree) => return Ok(tree),
            Self::Recovering(tree) => {
                let l1_batch = snapshot_l1_batch(pool)
                    .await?
                    .context("Merkle tree is recovering, but Postgres doesn't contain")?;
                let recovered_version = tree.recovered_version();
                anyhow::ensure!(
                    u64::from(l1_batch.0) == recovered_version,
                    "Snapshot L1 batch in Postgres ({l1_batch}) differs from the recovered Merkle tree version \
                     ({recovered_version})"
                );
                tree
            }
            Self::Empty { db, mode } => {
                if let Some(l1_batch) = snapshot_l1_batch(pool).await? {
                    AsyncTreeRecovery::new(db, l1_batch.0.into())
                } else {
                    // Start the tree from scratch. The genesis block will be filled in `TreeUpdater::loop_updating_tree()`.
                    return Ok(AsyncTree::new(db, mode));
                }
            }
        };
        todo!()
    }
}

async fn snapshot_l1_batch(_pool: &ConnectionPool) -> anyhow::Result<Option<L1BatchNumber>> {
    Ok(None)
}
