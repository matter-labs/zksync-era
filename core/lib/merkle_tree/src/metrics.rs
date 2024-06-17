//! Storage-related Merkle tree metrics. All metrics code in the crate should be in this module.

use std::{
    fmt, ops,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use vise::{
    Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Global, Histogram, Metrics, Unit,
};

use crate::types::Nibbles;

#[derive(Debug, Metrics)]
#[metrics(prefix = "merkle_tree")]
pub(crate) struct GeneralMetrics {
    /// Current number of leaves in the tree.
    pub leaf_count: Gauge<u64>,
}

#[vise::register]
pub(crate) static GENERAL_METRICS: Global<GeneralMetrics> = Global::new();

const BYTE_SIZE_BUCKETS: Buckets = Buckets::exponential(65_536.0..=16.0 * 1_024.0 * 1_024.0, 2.0);

#[derive(Debug, Metrics)]
#[metrics(prefix = "merkle_tree_finalize_patch")]
struct HashingMetrics {
    /// Total amount of hashing input performed while processing a patch.
    #[metrics(buckets = BYTE_SIZE_BUCKETS, unit = Unit::Bytes)]
    hashed: Histogram<u64>,
    /// Total time spent on hashing while processing a patch.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    hashing_duration: Histogram<Duration>,
}

/// Hashing-related statistics reported as metrics for each block of operations.
#[derive(Debug, Default)]
#[must_use = "hashing stats should be `report()`ed"]
pub(crate) struct HashingStats {
    pub hashed_bytes: AtomicU64,
    pub hashing_duration: Duration,
}

impl HashingStats {
    pub fn add_hashed_bytes(&self, bytes: u64) {
        self.hashed_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn report(self) {
        #[vise::register]
        static HASHING_METRICS: Global<HashingMetrics> = Global::new();

        let hashed_bytes = self.hashed_bytes.into_inner();
        HASHING_METRICS.hashed.observe(hashed_bytes);
        HASHING_METRICS
            .hashing_duration
            .observe(self.hashing_duration);
    }
}

const NODE_COUNT_BUCKETS: Buckets = Buckets::linear(1_000.0..=10_000.0, 1_000.0);
const LEAF_LEVEL_BUCKETS: Buckets = Buckets::linear(20.0..=40.0, 4.0);

#[derive(Debug, Metrics)]
#[metrics(prefix = "merkle_tree_extend_patch")]
struct TreeUpdateMetrics {
    // Metrics related to the AR16MT tree architecture
    /// Number of new leaves inserted during tree traversal while processing a single batch.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    new_leaves: Histogram<u64>,
    /// Number of new internal nodes inserted during tree traversal while processing a single batch.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    new_internal_nodes: Histogram<u64>,
    /// Number of existing leaves moved to a new location while processing a single batch.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    moved_leaves: Histogram<u64>,
    /// Number of existing leaves updated while processing a single batch.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    updated_leaves: Histogram<u64>,
    /// Average level of leaves moved or created while processing a single batch.
    #[metrics(buckets = LEAF_LEVEL_BUCKETS)]
    avg_leaf_level: Histogram<f64>,
    /// Maximum level of leaves moved or created while processing a single batch.
    #[metrics(buckets = LEAF_LEVEL_BUCKETS)]
    max_leaf_level: Histogram<u64>,

    // Metrics related to input instructions
    /// Number of keys read while processing a single batch (only applicable to the full operation mode).
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    key_reads: Histogram<u64>,
    /// Number of missing keys read while processing a single batch (only applicable to the full
    /// operation mode).
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    missing_key_reads: Histogram<u64>,
    /// Number of nodes of previous versions read from the DB while processing a single batch.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    db_reads: Histogram<u64>,
    /// Number of nodes of the current version re-read from the patch set while processing a single batch.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    patch_reads: Histogram<u64>,
}

#[vise::register]
static TREE_UPDATE_METRICS: Global<TreeUpdateMetrics> = Global::new();

#[must_use = "tree updater stats should be `report()`ed"]
#[derive(Clone, Copy, Default)]
pub(crate) struct TreeUpdaterStats {
    pub new_leaves: u64,
    pub new_internal_nodes: u64,
    pub moved_leaves: u64,
    pub updated_leaves: u64,
    pub leaf_level_sum: u64,
    pub max_leaf_level: u64,
    pub key_reads: u64,
    pub missing_key_reads: u64,
    pub db_reads: u64,
    pub patch_reads: u64,
}

impl fmt::Debug for TreeUpdaterStats {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TreeUpdaterStats")
            .field("new_leaves", &self.new_leaves)
            .field("new_internal_nodes", &self.new_internal_nodes)
            .field("moved_leaves", &self.moved_leaves)
            .field("updated_leaves", &self.updated_leaves)
            .field("avg_leaf_level", &self.avg_leaf_level())
            .field("max_leaf_level", &self.max_leaf_level)
            .field("key_reads", &self.key_reads)
            .field("missing_key_reads", &self.missing_key_reads)
            .field("db_reads", &self.db_reads)
            .field("patch_reads", &self.patch_reads)
            .finish_non_exhaustive()
    }
}

impl TreeUpdaterStats {
    pub(crate) fn update_leaf_levels(&mut self, nibble_count: usize) {
        let leaf_level = nibble_count as u64 * 4;
        self.leaf_level_sum += leaf_level;
        self.max_leaf_level = self.max_leaf_level.max(leaf_level);
    }

    #[allow(clippy::cast_precision_loss)] // Acceptable for metrics
    fn avg_leaf_level(&self) -> f64 {
        let touched_leaves = self.new_leaves + self.moved_leaves;
        if touched_leaves > 0 {
            self.leaf_level_sum as f64 / touched_leaves as f64
        } else {
            0.0
        }
    }

    pub(crate) fn report(self) {
        let metrics = &TREE_UPDATE_METRICS;
        metrics.new_leaves.observe(self.new_leaves);
        metrics.new_internal_nodes.observe(self.new_internal_nodes);
        metrics.moved_leaves.observe(self.moved_leaves);
        metrics.updated_leaves.observe(self.updated_leaves);
        metrics.avg_leaf_level.observe(self.avg_leaf_level());
        metrics.max_leaf_level.observe(self.max_leaf_level);

        if self.key_reads > 0 {
            metrics.key_reads.observe(self.key_reads);
        }
        if self.missing_key_reads > 0 {
            metrics.key_reads.observe(self.missing_key_reads);
        }
        metrics.db_reads.observe(self.db_reads);
        metrics.patch_reads.observe(self.patch_reads);
    }
}

impl ops::AddAssign for TreeUpdaterStats {
    fn add_assign(&mut self, rhs: Self) {
        self.new_leaves += rhs.new_leaves;
        self.new_internal_nodes += rhs.new_internal_nodes;
        self.moved_leaves += rhs.moved_leaves;
        self.updated_leaves += rhs.updated_leaves;
        self.leaf_level_sum += rhs.leaf_level_sum;
        self.max_leaf_level = self.max_leaf_level.max(rhs.max_leaf_level);

        self.key_reads += rhs.key_reads;
        self.missing_key_reads += rhs.missing_key_reads;
        self.db_reads += rhs.db_reads;
        self.patch_reads += rhs.patch_reads;
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "merkle_tree")]
pub(crate) struct BlockTimings {
    /// Time spent loading tree nodes from DB per batch.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub load_nodes: Histogram<Duration>,
    /// Time spent traversing the tree and creating new nodes per batch.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub extend_patch: Histogram<Duration>,
    /// Time spent finalizing a batch (mainly hash computations).
    #[metrics(buckets = Buckets::LATENCIES)]
    pub finalize_patch: Histogram<Duration>,
}

#[vise::register]
pub(crate) static BLOCK_TIMINGS: Global<BlockTimings> = Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "nibbles")]
struct NibbleCount(usize);

const MAX_TRACKED_NIBBLE_COUNT: usize = 6;

impl NibbleCount {
    fn new(raw_count: usize) -> Self {
        Self(raw_count.min(MAX_TRACKED_NIBBLE_COUNT))
    }
}

impl fmt::Display for NibbleCount {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 == MAX_TRACKED_NIBBLE_COUNT {
            write!(formatter, "{MAX_TRACKED_NIBBLE_COUNT}..")
        } else {
            write!(formatter, "{}", self.0)
        }
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "merkle_tree_apply_patch")]
struct ApplyPatchMetrics {
    /// Total number of nodes included into a RocksDB patch per batch.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    nodes: Histogram<u64>,
    /// Number of nodes included into a RocksDB patch per batch, grouped by the key nibble count.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    nodes_by_nibble_count: Family<NibbleCount, Histogram<u64>>,
    /// Total byte size of nodes included into a RocksDB patch per batch, grouped by the key nibble count.
    #[metrics(buckets = BYTE_SIZE_BUCKETS)]
    node_bytes: Family<NibbleCount, Histogram<u64>>,
    /// Number of hashes in child references copied from previous tree versions. Allows to estimate
    /// the level of redundancy of the tree.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    copied_hashes: Histogram<u64>,
}

#[vise::register]
static APPLY_PATCH_METRICS: Global<ApplyPatchMetrics> = Global::new();

#[derive(Debug, Default, Clone, Copy)]
struct PatchNodeStats {
    /// Number of nodes.
    count: u64,
    /// Total serialized size of nodes (excluding key sizes).
    bytes: u64,
}

#[must_use = "patch metrics should be `report()`ed"]
#[derive(Debug)]
pub(crate) struct ApplyPatchStats {
    node_stats_by_nibble_count: [PatchNodeStats; MAX_TRACKED_NIBBLE_COUNT + 1],
    copied_hashes: u64,
}

impl ApplyPatchStats {
    pub fn new(copied_hashes: u64) -> Self {
        Self {
            node_stats_by_nibble_count: [PatchNodeStats::default(); MAX_TRACKED_NIBBLE_COUNT + 1],
            copied_hashes,
        }
    }

    pub fn update_node_bytes(&mut self, key_nibbles: &Nibbles, node_bytes: &[u8]) {
        let nibble_count = key_nibbles.nibble_count();
        let idx = nibble_count.min(MAX_TRACKED_NIBBLE_COUNT);
        let stats = &mut self.node_stats_by_nibble_count[idx];
        stats.count += 1;
        stats.bytes += node_bytes.len() as u64;
    }

    pub fn report(self) {
        let metrics = &APPLY_PATCH_METRICS;
        let total_node_count = self
            .node_stats_by_nibble_count
            .iter()
            .map(|stats| stats.count)
            .sum::<u64>();
        metrics.nodes.observe(total_node_count);

        let node_bytes = self.node_stats_by_nibble_count.iter().enumerate();
        for (nibble_count, stats) in node_bytes {
            let label = NibbleCount::new(nibble_count);
            metrics.nodes_by_nibble_count[&label].observe(stats.count);
            metrics.node_bytes[&label].observe(stats.bytes);
        }

        metrics.copied_hashes.observe(self.copied_hashes);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "bound", rename_all = "snake_case")]
enum Bound {
    Start,
    End,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "merkle_tree_pruning")]
struct PruningMetrics {
    /// Minimum Merkle tree version targeted after a single pruning iteration. The iteration
    /// may not remove all stale keys to this version if there are too many.
    target_retained_version: Gauge<u64>,
    /// Number of pruned node keys on a specific pruning iteration.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    key_count: Histogram<usize>,
    /// Lower and upper boundaries on the new stale key versions deleted
    /// during a pruning iteration. The lower boundary is inclusive, the upper one is exclusive.
    deleted_stale_key_versions: Family<Bound, Gauge<u64>>,
}

#[vise::register]
static PRUNING_METRICS: Global<PruningMetrics> = Global::new();

#[derive(Debug)]
pub struct PruningStats {
    pub target_retained_version: u64,
    pub pruned_key_count: usize,
    pub deleted_stale_key_versions: ops::Range<u64>,
}

impl PruningStats {
    pub fn report(&self) {
        PRUNING_METRICS
            .target_retained_version
            .set(self.target_retained_version);
        PRUNING_METRICS.key_count.observe(self.pruned_key_count);
        PRUNING_METRICS.deleted_stale_key_versions[&Bound::Start]
            .set(self.deleted_stale_key_versions.start);
        PRUNING_METRICS.deleted_stale_key_versions[&Bound::End]
            .set(self.deleted_stale_key_versions.end);
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "merkle_tree_pruning")]
pub(crate) struct PruningTimings {
    /// Time spent loading stale keys per pruning iteration.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub load_stale_keys: Histogram<Duration>,
    /// Time spent removing stale keys from RocksDB per pruning iteration.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub apply_patch: Histogram<Duration>,
}

#[vise::register]
pub(crate) static PRUNING_TIMINGS: Global<PruningTimings> = Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(crate) enum RecoveryStage {
    Extend,
    ApplyPatch,
    ParallelPersistence,
}

const CHUNK_SIZE_BUCKETS: Buckets = Buckets::values(&[
    1_000.0,
    2_000.0,
    5_000.0,
    10_000.0,
    20_000.0,
    50_000.0,
    100_000.0,
    200_000.0,
    500_000.0,
    1_000_000.0,
    2_000_000.0,
    5_000_000.0,
]);

#[derive(Debug, Metrics)]
#[metrics(prefix = "merkle_tree_recovery")]
pub(crate) struct RecoveryMetrics {
    /// Number of entries in a recovered chunk.
    #[metrics(buckets = CHUNK_SIZE_BUCKETS)]
    pub chunk_size: Histogram<usize>,
    /// Latency of a specific stage of recovery for a single chunk.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub stage_latency: Family<RecoveryStage, Histogram<Duration>>,
    /// Number of buffered commands if parallel persistence is used.
    pub parallel_persistence_buffer_size: Gauge<usize>,
}

#[vise::register]
pub(crate) static RECOVERY_METRICS: Global<RecoveryMetrics> = Global::new();
