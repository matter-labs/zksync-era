//! This module applies updates to the ZkSyncTree, calculates metadata for sealed blocks, and
//! stores them in the DB.

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use tokio::sync::watch;
use zksync_config::configs::{
    chain::OperationsManagerConfig,
    database::{MerkleTreeConfig, MerkleTreeMode},
};
use zksync_dal::{ConnectionPool, Core};
use zksync_health_check::{HealthUpdater, ReactiveHealthCheck};
use zksync_object_store::ObjectStore;

pub use self::helpers::LazyAsyncTreeReader;
pub(crate) use self::helpers::{AsyncTreeReader, L1BatchWithLogs, MerkleTreeInfo};
use self::{
    helpers::{create_db, Delayer, GenericAsyncTree, MerkleTreeHealth},
    updater::TreeUpdater,
};

mod helpers;
mod metrics;
mod recovery;
#[cfg(test)]
pub(crate) mod tests;
mod updater;

/// Configuration of [`MetadataCalculator`].
#[derive(Debug)]
pub struct MetadataCalculatorConfig {
    /// Filesystem path to the RocksDB instance that stores the tree.
    pub db_path: String,
    /// Configuration of the Merkle tree mode.
    pub mode: MerkleTreeMode,
    /// Interval between polling Postgres for updates if no progress was made by the tree.
    pub delay_interval: Duration,
    /// Maximum number of L1 batches to get from Postgres on a single update iteration.
    pub max_l1_batches_per_iter: usize,
    /// Chunk size for multi-get operations. Can speed up loading data for the Merkle tree on some environments,
    /// but the effects vary wildly depending on the setup (e.g., the filesystem used).
    pub multi_get_chunk_size: usize,
    /// Capacity of RocksDB block cache in bytes. Reasonable values range from ~100 MiB to several GB.
    pub block_cache_capacity: usize,
    /// Capacity of RocksDB memtables. Can be set to a reasonably large value (order of 512 MiB)
    /// to mitigate write stalls.
    pub memtable_capacity: usize,
    /// Timeout to wait for the Merkle tree database to run compaction on stalled writes.
    pub stalled_writes_timeout: Duration,
}

impl MetadataCalculatorConfig {
    pub fn for_main_node(
        merkle_tree_config: &MerkleTreeConfig,
        operation_config: &OperationsManagerConfig,
    ) -> Self {
        Self {
            db_path: merkle_tree_config.path.clone(),
            mode: merkle_tree_config.mode,
            delay_interval: operation_config.delay_interval(),
            max_l1_batches_per_iter: merkle_tree_config.max_l1_batches_per_iter,
            multi_get_chunk_size: merkle_tree_config.multi_get_chunk_size,
            block_cache_capacity: merkle_tree_config.block_cache_size(),
            memtable_capacity: merkle_tree_config.memtable_capacity(),
            stalled_writes_timeout: merkle_tree_config.stalled_writes_timeout(),
        }
    }
}

#[derive(Debug)]
pub struct MetadataCalculator {
    config: MetadataCalculatorConfig,
    tree_reader: watch::Sender<Option<AsyncTreeReader>>,
    object_store: Option<Arc<dyn ObjectStore>>,
    delayer: Delayer,
    health_updater: HealthUpdater,
    max_l1_batches_per_iter: usize,
}

impl MetadataCalculator {
    /// Creates a calculator with the specified `config`.
    pub async fn new(
        config: MetadataCalculatorConfig,
        object_store: Option<Arc<dyn ObjectStore>>,
    ) -> anyhow::Result<Self> {
        anyhow::ensure!(
            config.max_l1_batches_per_iter > 0,
            "Maximum L1 batches per iteration is misconfigured to be 0; please update it to positive value"
        );
        if matches!(config.mode, MerkleTreeMode::Lightweight) && object_store.is_some() {
            anyhow::bail!(
                "Cannot run lightweight tree with an object store; the tree won't produce information to be stored in the store"
            );
        }

        let (_, health_updater) = ReactiveHealthCheck::new("tree");
        Ok(Self {
            tree_reader: watch::channel(None).0,
            object_store,
            delayer: Delayer::new(config.delay_interval),
            health_updater,
            max_l1_batches_per_iter: config.max_l1_batches_per_iter,
            config,
        })
    }

    /// Returns a health check for this calculator.
    pub fn tree_health_check(&self) -> ReactiveHealthCheck {
        self.health_updater.subscribe()
    }

    /// Returns a reference to the tree reader.
    pub fn tree_reader(&self) -> LazyAsyncTreeReader {
        LazyAsyncTreeReader(self.tree_reader.subscribe())
    }

    async fn create_tree(&self) -> anyhow::Result<GenericAsyncTree> {
        self.health_updater
            .update(MerkleTreeHealth::Initialization.into());

        let started_at = Instant::now();
        let db = create_db(
            self.config.db_path.clone().into(),
            self.config.block_cache_capacity,
            self.config.memtable_capacity,
            self.config.stalled_writes_timeout,
            self.config.multi_get_chunk_size,
        )
        .await
        .with_context(|| {
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

        Ok(GenericAsyncTree::new(db, self.config.mode).await)
    }

    pub async fn run(
        self,
        pool: ConnectionPool<Core>,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let tree = self.create_tree().await?;
        let tree = tree
            .ensure_ready(&pool, &stop_receiver, &self.health_updater)
            .await?;
        let Some(tree) = tree else {
            return Ok(()); // recovery was aborted because a stop signal was received
        };
        let tree_reader = tree.reader();
        tracing::info!(
            "Merkle tree is initialized and ready to process L1 batches: {:?}",
            tree_reader.clone().info().await
        );
        self.tree_reader.send_replace(Some(tree_reader));

        let updater = TreeUpdater::new(tree, self.max_l1_batches_per_iter, self.object_store);
        updater
            .loop_updating_tree(self.delayer, &pool, stop_receiver, self.health_updater)
            .await
    }
}
