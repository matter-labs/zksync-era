//! Merkle tree pruning logic.

use std::time::Duration;

use anyhow::Context as _;
use tokio::sync::{oneshot, watch};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_merkle_tree::{MerkleTreePruner, MerkleTreePrunerHandle, RocksDBWrapper};

pub(super) type PruningHandles = (MerkleTreePruner<RocksDBWrapper>, MerkleTreePrunerHandle);

/// Task performing Merkle tree pruning according to the pruning entries in Postgres.
#[derive(Debug)]
#[must_use = "Task should `run()` in a managed Tokio task"]
pub struct MerkleTreePruningTask {
    handles: oneshot::Receiver<PruningHandles>,
    pool: ConnectionPool<Core>,
    poll_interval: Duration,
}

impl MerkleTreePruningTask {
    pub(super) fn new(
        handles: oneshot::Receiver<PruningHandles>,
        pool: ConnectionPool<Core>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            handles,
            pool,
            poll_interval,
        }
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let (pruner, pruner_handle);
        tokio::select! {
            res = self.handles => {
                match res {
                    Ok(handles) => (pruner, pruner_handle) = handles,
                    Err(_) => {
                        tracing::info!("Merkle tree dropped; shutting down tree pruning");
                        return Ok(());
                    }
                }
            }
            _ = stop_receiver.changed() => {
                tracing::info!("Stop signal received before Merkle tree is initialized; shutting down tree pruning");
                return Ok(());
            }
        }
        tracing::info!("Obtained pruning handles; starting Merkle tree pruning");

        // FIXME: is this good enough (vs a managed task)?
        let pruner_task_handle = tokio::task::spawn_blocking(|| pruner.run());

        while !*stop_receiver.borrow_and_update() {
            let mut storage = self.pool.connection_tagged("metadata_calculator").await?;
            let l1_batch_number = storage
                .blocks_dal()
                .get_sealed_l1_batch_number()
                .await?
                .unwrap(); // FIXME: get actual pruning data
            drop(storage);

            pruner_handle.set_target_retained_version(l1_batch_number.0.into());

            if tokio::time::timeout(self.poll_interval, stop_receiver.changed())
                .await
                .is_ok()
            {
                break;
            }
        }

        tracing::info!("Stop signal received, Merkle tree pruning is shutting down");
        pruner_handle.abort();
        pruner_task_handle
            .await
            .context("Merkle tree pruning thread panicked")
    }
}
