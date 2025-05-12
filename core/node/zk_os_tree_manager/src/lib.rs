use std::{
    num::{NonZeroU32, NonZeroUsize},
    path::PathBuf,
    time::{Duration, Instant},
};

use anyhow::Context;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_health_check::{CheckHealth, HealthUpdater, ReactiveHealthCheck};
use zksync_shared_metrics::tree::{ConfigLabels, ModeLabel, METRICS};
use zksync_types::try_stoppable;
#[cfg(test)]
use zksync_types::L1BatchNumber;

pub use self::helpers::LazyAsyncTreeReader;
use crate::{
    health::{MerkleTreeHealth, MerkleTreeHealthCheck},
    helpers::{create_db, AsyncMerkleTree, AsyncTreeReader},
    updater::TreeUpdater,
};

pub mod api;
mod batch;
mod health;
mod helpers;
#[cfg(test)]
mod tests;
mod updater;

/// Configuration of [`TreeManager`].
#[derive(Debug, Clone)]
pub struct TreeManagerConfig {
    /// Filesystem path to the RocksDB instance that stores the tree.
    pub db_path: PathBuf,
    /// Maximum number of files concurrently opened by RocksDB. Useful to fit into OS limits; can be used
    /// as a rudimentary way to control RAM usage of the tree.
    pub max_open_files: Option<NonZeroU32>,
    /// Interval between polling Postgres for updates if no progress was made by the tree.
    pub delay_interval: Duration,
    /// Maximum number of L1 batches to get from Postgres on a single update iteration.
    pub max_l1_batches_per_iter: NonZeroUsize,
    /// Chunk size for multi-get operations. Can speed up loading data for the Merkle tree on some environments,
    /// but the effects vary wildly depending on the setup (e.g., the filesystem used).
    pub multi_get_chunk_size: usize,
    /// Capacity of RocksDB block cache in bytes. Reasonable values range from ~100 MiB to several GB.
    pub block_cache_capacity: usize,
    /// If specified, RocksDB indices and Bloom filters will be managed by the block cache, rather than
    /// being loaded entirely into RAM on the RocksDB initialization. The block cache capacity should be increased
    /// correspondingly; otherwise, RocksDB performance can significantly degrade.
    pub include_indices_and_filters_in_block_cache: bool,
}

impl TreeManagerConfig {
    const MEMTABLE_CAPACITY: usize = 256 << 20;
    const STALLED_WRITES_TIMEOUT: Duration = Duration::from_secs(30);

    fn as_labels(&self) -> ConfigLabels {
        ConfigLabels {
            mode: ModeLabel::Lightweight,
            delay_interval: self.delay_interval.into(),
            max_l1_batches_per_iter: self.max_l1_batches_per_iter.get(),
            multi_get_chunk_size: self.multi_get_chunk_size,
            block_cache_capacity: self.block_cache_capacity,
            memtable_capacity: Self::MEMTABLE_CAPACITY,
            stalled_writes_timeout: Self::STALLED_WRITES_TIMEOUT.into(),
        }
    }
}

#[derive(Debug)]
pub struct TreeManager {
    config: TreeManagerConfig,
    pool: ConnectionPool<Core>,
    health_updater: HealthUpdater,
    tree_reader: watch::Sender<Option<AsyncTreeReader>>,
    #[cfg(test)]
    next_l1_batch_sender: watch::Sender<L1BatchNumber>,
}

impl TreeManager {
    pub fn new(config: TreeManagerConfig, pool: ConnectionPool<Core>) -> Self {
        if let Err(err) = METRICS.info.set(config.as_labels()) {
            tracing::warn!(
                "Cannot set config {:?}; it's already set to {:?}",
                err.into_inner(),
                METRICS.info.get()
            );
        }

        let (_, health_updater) = ReactiveHealthCheck::new("tree");
        Self {
            config,
            pool,
            health_updater,
            tree_reader: watch::channel(None).0,
            #[cfg(test)]
            next_l1_batch_sender: watch::channel(L1BatchNumber(0)).0,
        }
    }

    /// Returns a health check for the tree.
    pub fn tree_health_check(&self) -> impl CheckHealth {
        MerkleTreeHealthCheck::new(self.health_updater.subscribe(), self.tree_reader())
    }

    /// Returns a reference to the tree reader.
    pub fn tree_reader(&self) -> LazyAsyncTreeReader {
        LazyAsyncTreeReader(self.tree_reader.subscribe())
    }

    #[cfg(test)]
    fn subscribe_to_l1_batches(&self) -> watch::Receiver<L1BatchNumber> {
        self.next_l1_batch_sender.subscribe()
    }

    async fn create_tree(&self) -> anyhow::Result<AsyncMerkleTree> {
        self.health_updater
            .update(MerkleTreeHealth::Initialization.into());

        let started_at = Instant::now();
        let db = create_db(self.config.clone()).await.with_context(|| {
            format!(
                "failed opening Merkle tree RocksDB with configuration {:?}",
                self.config
            )
        })?;
        tracing::info!(
            "Opened Merkle tree RocksDB with configuration {:?} in {:?}",
            self.config,
            started_at.elapsed()
        );

        AsyncMerkleTree::new(db).await
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let mut tree = self.create_tree().await?;

        // Set a tree reader before the tree is fully initialized to not wait for the first L1 batch to appear in Postgres.
        let tree_reader = tree.reader();
        self.tree_reader.send_replace(Some(tree_reader));

        try_stoppable!(
            tree.ensure_consistency(self.config.delay_interval, &self.pool, &mut stop_receiver)
                .await
        );

        let tree_info = tree.reader().info().await.context("cannot get tree info")?;
        tracing::info!("Merkle tree is initialized and ready to process L1 batches: {tree_info:?}");
        self.health_updater
            .update(MerkleTreeHealth::MainLoop(tree_info).into());

        let updater = TreeUpdater {
            tree,
            max_l1_batches_per_iter: self.config.max_l1_batches_per_iter.get(),
            #[cfg(test)]
            next_l1_batch_sender: self.next_l1_batch_sender,
        };
        updater
            .loop_updating_tree(self.config.delay_interval, &self.pool, stop_receiver)
            .await
    }
}
