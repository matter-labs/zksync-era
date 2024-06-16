//! This module applies updates to the ZkSyncTree, calculates metadata for sealed blocks, and
//! stores them in the DB.

use std::{
    num::{NonZeroU32, NonZeroUsize},
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context as _;
use tokio::sync::{oneshot, watch};
use zksync_config::configs::{
    chain::OperationsManagerConfig,
    database::{MerkleTreeConfig, MerkleTreeMode},
};
use zksync_dal::{ConnectionPool, Core};
use zksync_health_check::{CheckHealth, HealthUpdater, ReactiveHealthCheck};
use zksync_object_store::ObjectStore;

use self::{
    helpers::{create_db, Delayer, GenericAsyncTree, MerkleTreeHealth, MerkleTreeHealthCheck},
    metrics::{ConfigLabels, METRICS},
    pruning::PruningHandles,
    updater::TreeUpdater,
};
pub use self::{
    helpers::{AsyncTreeReader, LazyAsyncTreeReader, MerkleTreeInfo},
    pruning::MerkleTreePruningTask,
};

pub mod api_server;
mod helpers;
mod metrics;
mod pruning;
mod recovery;
#[cfg(test)]
pub(crate) mod tests;
mod updater;

#[derive(Debug, Clone)]
pub struct MetadataCalculatorRecoveryConfig {
    /// Approximate chunk size (measured in the number of entries) to recover on a single iteration.
    /// Reasonable values are order of 100,000 (meaning an iteration takes several seconds).
    ///
    /// **Important.** This value cannot be changed in the middle of tree recovery (i.e., if a node is stopped in the middle
    /// of recovery and then restarted with a different config).
    pub desired_chunk_size: u64,
    /// Buffer capacity for parallel persistence operations. Should be reasonably small since larger buffer means more RAM usage;
    /// buffer elements are persisted tree chunks. OTOH, small buffer can lead to persistence parallelization being inefficient.
    ///
    /// If set to `None`, parallel persistence will be disabled.
    pub parallel_persistence_buffer: Option<NonZeroUsize>,
}

impl Default for MetadataCalculatorRecoveryConfig {
    fn default() -> Self {
        Self {
            desired_chunk_size: 200_000,
            parallel_persistence_buffer: NonZeroUsize::new(4),
        }
    }
}

/// Configuration of [`MetadataCalculator`].
#[derive(Debug, Clone)]
pub struct MetadataCalculatorConfig {
    /// Filesystem path to the RocksDB instance that stores the tree.
    pub db_path: String,
    /// Maximum number of files concurrently opened by RocksDB. Useful to fit into OS limits; can be used
    /// as a rudimentary way to control RAM usage of the tree.
    pub max_open_files: Option<NonZeroU32>,
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
    /// If specified, RocksDB indices and Bloom filters will be managed by the block cache, rather than
    /// being loaded entirely into RAM on the RocksDB initialization. The block cache capacity should be increased
    /// correspondingly; otherwise, RocksDB performance can significantly degrade.
    pub include_indices_and_filters_in_block_cache: bool,
    /// Capacity of RocksDB memtables. Can be set to a reasonably large value (order of 512 MiB)
    /// to mitigate write stalls.
    pub memtable_capacity: usize,
    /// Timeout to wait for the Merkle tree database to run compaction on stalled writes.
    pub stalled_writes_timeout: Duration,
    /// Configuration specific to the Merkle tree recovery.
    pub recovery: MetadataCalculatorRecoveryConfig,
}

impl MetadataCalculatorConfig {
    pub fn for_main_node(
        merkle_tree_config: &MerkleTreeConfig,
        operation_config: &OperationsManagerConfig,
    ) -> Self {
        Self {
            db_path: merkle_tree_config.path.clone(),
            max_open_files: None,
            mode: merkle_tree_config.mode,
            delay_interval: operation_config.delay_interval(),
            max_l1_batches_per_iter: merkle_tree_config.max_l1_batches_per_iter,
            multi_get_chunk_size: merkle_tree_config.multi_get_chunk_size,
            block_cache_capacity: merkle_tree_config.block_cache_size(),
            include_indices_and_filters_in_block_cache: false,
            memtable_capacity: merkle_tree_config.memtable_capacity(),
            stalled_writes_timeout: merkle_tree_config.stalled_writes_timeout(),
            // The main node isn't supposed to be recovered yet, so this value doesn't matter much
            recovery: MetadataCalculatorRecoveryConfig::default(),
        }
    }
}

#[derive(Debug)]
pub struct MetadataCalculator {
    config: MetadataCalculatorConfig,
    tree_reader: watch::Sender<Option<AsyncTreeReader>>,
    pruning_handles_sender: oneshot::Sender<PruningHandles>,
    object_store: Option<Arc<dyn ObjectStore>>,
    pool: ConnectionPool<Core>,
    recovery_pool: ConnectionPool<Core>,
    delayer: Delayer,
    health_updater: HealthUpdater,
    max_l1_batches_per_iter: usize,
}

impl MetadataCalculator {
    /// Creates a calculator with the specified `config`.
    ///
    /// # Arguments
    ///
    /// - `pool` can have a single connection (but then you should set a separate recovery pool).
    pub async fn new(
        config: MetadataCalculatorConfig,
        object_store: Option<Arc<dyn ObjectStore>>,
        pool: ConnectionPool<Core>,
    ) -> anyhow::Result<Self> {
        if let Err(err) = METRICS.info.set(ConfigLabels::new(&config)) {
            tracing::warn!(
                "Cannot set config {:?}; it's already set to {:?}",
                err.into_inner(),
                METRICS.info.get()
            );
        }

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
            pruning_handles_sender: oneshot::channel().0,
            object_store,
            recovery_pool: pool.clone(),
            pool,
            delayer: Delayer::new(config.delay_interval),
            health_updater,
            max_l1_batches_per_iter: config.max_l1_batches_per_iter,
            config,
        })
    }

    /// Sets a separate pool that will be used in case of snapshot recovery. It should have multiple connections
    /// (e.g., 10) to speed up recovery.
    pub fn with_recovery_pool(mut self, recovery_pool: ConnectionPool<Core>) -> Self {
        self.recovery_pool = recovery_pool;
        self
    }

    /// Returns a health check for this calculator.
    pub fn tree_health_check(&self) -> impl CheckHealth {
        MerkleTreeHealthCheck::new(self.health_updater.subscribe(), self.tree_reader())
    }

    /// Returns a reference to the tree reader.
    pub fn tree_reader(&self) -> LazyAsyncTreeReader {
        LazyAsyncTreeReader(self.tree_reader.subscribe())
    }

    /// Returns a task that can be used to prune the Merkle tree according to the pruning logs in Postgres.
    /// This method should be called once; only the latest returned task will do any job, all previous ones
    /// will terminate immediately.
    pub fn pruning_task(&mut self, poll_interval: Duration) -> MerkleTreePruningTask {
        let (pruning_handles_sender, pruning_handles) = oneshot::channel();
        self.pruning_handles_sender = pruning_handles_sender;
        MerkleTreePruningTask::new(pruning_handles, self.pool.clone(), poll_interval)
    }

    async fn create_tree(&self) -> anyhow::Result<GenericAsyncTree> {
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

        GenericAsyncTree::new(db, &self.config).await
    }

    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let tree = self.create_tree().await?;
        let tree = tree
            .ensure_ready(
                &self.config.recovery,
                &self.pool,
                self.recovery_pool,
                &self.health_updater,
                &stop_receiver,
            )
            .await?;
        let Some(mut tree) = tree else {
            return Ok(()); // recovery was aborted because a stop signal was received
        };
        // Set a tree reader before the tree is fully initialized to not wait for the first L1 batch to appear in Postgres.
        let tree_reader = tree.reader();
        self.tree_reader.send_replace(Some(tree_reader));

        tree.ensure_consistency(&self.delayer, &self.pool, &mut stop_receiver)
            .await?;
        if !self.pruning_handles_sender.is_closed() {
            // Unlike tree reader, we shouldn't initialize pruning (as a task modifying the tree) before the tree is guaranteed
            // to be consistent with Postgres.
            self.pruning_handles_sender.send(tree.pruner()).ok();
        }

        let tree_info = tree.reader().info().await;
        tracing::info!("Merkle tree is initialized and ready to process L1 batches: {tree_info:?}");
        self.health_updater
            .update(MerkleTreeHealth::MainLoop(tree_info).into());

        let updater = TreeUpdater::new(tree, self.max_l1_batches_per_iter, self.object_store);
        updater
            .loop_updating_tree(self.delayer, &self.pool, stop_receiver)
            .await
    }
}
