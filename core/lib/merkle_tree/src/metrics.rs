//! Storage-related Merkle tree metrics. All metrics code in the crate should be in this module.

#![allow(clippy::cast_precision_loss)]
// ^ Casting `u64` to `f64` shouldn't practically result in precision loss in most cases,
// and we're OK with precision loss for metrics in general.

use metrics::Unit;

use std::{
    ops,
    sync::{
        atomic::{AtomicU64, Ordering},
        Once,
    },
    time::Instant,
};

use crate::types::Nibbles;

/// Describes all metrics in the crate once.
pub(crate) fn describe_metrics() {
    static INITIALIZER: Once = Once::new();

    INITIALIZER.call_once(|| {
        HashingMetrics::describe();
        TreeUpdaterMetrics::describe();
        BlockTimings::describe();
        LeafCountMetric::describe();
        ApplyPatchMetrics::describe();
        PruningStats::describe();
        PruningTimings::describe();
    });
}

/// Hashing-related statistics reported as metrics for each block of operations.
#[derive(Debug, Default)]
#[must_use = "hashing metrics should be `report()`ed"]
pub(crate) struct HashingMetrics {
    pub hashed_bytes: AtomicU64,
}

impl HashingMetrics {
    const HASHED_BYTES: &'static str = "merkle_tree.finalize_patch.hashed_bytes";

    fn describe() {
        metrics::describe_histogram!(
            Self::HASHED_BYTES,
            Unit::Bytes,
            "Total amount of hashing input performed while processing a single block"
        );
    }

    pub fn add_hashed_bytes(&self, bytes: u64) {
        self.hashed_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn report(self) {
        let hashed_bytes = self.hashed_bytes.into_inner();
        metrics::histogram!(Self::HASHED_BYTES, hashed_bytes as f64);
    }
}

#[must_use = "tree updater metrics should be `report()`ed"]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct TreeUpdaterMetrics {
    // Metrics related to the AR16MT tree architecture
    pub new_leaves: u64,
    pub new_internal_nodes: u64,
    pub moved_leaves: u64,
    pub updated_leaves: u64,
    pub leaf_level_sum: u64,
    pub max_leaf_level: u64,
    // Metrics related to input instructions
    pub key_reads: u64,
    pub missing_key_reads: u64,
    pub db_reads: u64,
    pub patch_reads: u64,
}

impl TreeUpdaterMetrics {
    const NEW_LEAVES: &'static str = "merkle_tree.extend_patch.new_leaves";
    const NEW_INTERNAL_NODES: &'static str = "merkle_tree.extend_patch.new_internal_nodes";
    const MOVED_LEAVES: &'static str = "merkle_tree.extend_patch.moved_leaves";
    const UPDATED_LEAVES: &'static str = "merkle_tree.extend_patch.updated_leaves";
    const AVG_LEAF_LEVEL: &'static str = "merkle_tree.extend_patch.avg_leaf_level";
    const MAX_LEAF_LEVEL: &'static str = "merkle_tree.extend_patch.max_leaf_level";
    const KEY_READS: &'static str = "merkle_tree.extend_patch.key_reads";
    const MISSING_KEY_READS: &'static str = "merkle_tree.extend_patch.missing_key_reads";
    const DB_READS: &'static str = "merkle_tree.extend_patch.db_reads";
    const PATCH_READS: &'static str = "merkle_tree.extend_patch.patch_reads";

    fn describe() {
        metrics::describe_histogram!(
            Self::NEW_LEAVES,
            Unit::Count,
            "Number of new leaves inserted during tree traversal while processing a single block"
        );
        metrics::describe_histogram!(
            Self::NEW_INTERNAL_NODES,
            Unit::Count,
            "Number of new internal nodes inserted during tree traversal while processing \
             a single block"
        );
        metrics::describe_histogram!(
            Self::MOVED_LEAVES,
            Unit::Count,
            "Number of existing leaves moved to a new location while processing \
             a single block"
        );
        metrics::describe_histogram!(
            Self::UPDATED_LEAVES,
            Unit::Count,
            "Number of existing leaves updated while processing a single block"
        );
        metrics::describe_histogram!(
            Self::AVG_LEAF_LEVEL,
            Unit::Count,
            "Average level of leaves moved or created while processing a single block"
        );
        metrics::describe_histogram!(
            Self::MAX_LEAF_LEVEL,
            Unit::Count,
            "Maximum level of leaves moved or created while processing a single block"
        );

        metrics::describe_histogram!(
            Self::KEY_READS,
            Unit::Count,
            "Number of keys read while processing a single block (only applicable \
             to the full operation mode)"
        );
        metrics::describe_histogram!(
            Self::MISSING_KEY_READS,
            Unit::Count,
            "Number of missing keys read while processing a single block (only applicable \
             to the full operation mode)"
        );
        metrics::describe_histogram!(
            Self::DB_READS,
            Unit::Count,
            "Number of nodes of previous versions read from the DB while processing \
             a single block"
        );
        metrics::describe_histogram!(
            Self::PATCH_READS,
            Unit::Count,
            "Number of nodes of the current version re-read from the patch set while processing \
             a single block"
        );
    }

    pub(crate) fn update_leaf_levels(&mut self, nibble_count: usize) {
        let leaf_level = nibble_count as u64 * 4;
        self.leaf_level_sum += leaf_level;
        self.max_leaf_level = self.max_leaf_level.max(leaf_level);
    }

    pub(crate) fn report(self) {
        metrics::histogram!(Self::NEW_LEAVES, self.new_leaves as f64);
        metrics::histogram!(Self::NEW_INTERNAL_NODES, self.new_internal_nodes as f64);
        metrics::histogram!(Self::MOVED_LEAVES, self.moved_leaves as f64);
        metrics::histogram!(Self::UPDATED_LEAVES, self.updated_leaves as f64);

        let touched_leaves = self.new_leaves + self.moved_leaves;
        let avg_leaf_level = if touched_leaves > 0 {
            self.leaf_level_sum as f64 / touched_leaves as f64
        } else {
            0.0
        };
        metrics::histogram!(Self::AVG_LEAF_LEVEL, avg_leaf_level);
        metrics::histogram!(Self::MAX_LEAF_LEVEL, self.max_leaf_level as f64);

        if self.key_reads > 0 {
            metrics::histogram!(Self::KEY_READS, self.key_reads as f64);
        }
        if self.missing_key_reads > 0 {
            metrics::histogram!(Self::MISSING_KEY_READS, self.missing_key_reads as f64);
        }
        metrics::histogram!(Self::DB_READS, self.db_reads as f64);
        metrics::histogram!(Self::PATCH_READS, self.patch_reads as f64);
    }
}

impl ops::AddAssign for TreeUpdaterMetrics {
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

pub(crate) trait Timing: Copy {
    fn as_str(self) -> &'static str;

    fn start(self) -> TimingGuard<Self> {
        TimingGuard {
            metric: self,
            start: Instant::now(),
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum BlockTimings {
    LoadNodes,
    ExtendPatch,
    FinalizePatch,
}

impl BlockTimings {
    const LOAD_NODES: &'static str = "merkle_tree.load_nodes";
    const EXTEND_PATCH: &'static str = "merkle_tree.extend_patch";
    const FINALIZE_PATCH: &'static str = "merkle_tree.finalize_patch";

    fn describe() {
        metrics::describe_histogram!(
            Self::LOAD_NODES,
            Unit::Seconds,
            "Time spent loading tree nodes from DB per block"
        );
        metrics::describe_histogram!(
            Self::EXTEND_PATCH,
            Unit::Seconds,
            "Time spent traversing the tree and creating new nodes per block"
        );
        metrics::describe_histogram!(
            Self::FINALIZE_PATCH,
            Unit::Seconds,
            "Time spent finalizing the block (mainly hash computations)"
        );
    }
}

impl Timing for BlockTimings {
    fn as_str(self) -> &'static str {
        match self {
            Self::LoadNodes => Self::LOAD_NODES,
            Self::ExtendPatch => Self::EXTEND_PATCH,
            Self::FinalizePatch => Self::FINALIZE_PATCH,
        }
    }
}

#[must_use = "timings should be `report()`ed"]
#[derive(Debug)]
pub(crate) struct TimingGuard<T> {
    metric: T,
    start: Instant,
}

impl<T: Timing> TimingGuard<T> {
    pub fn report(self) {
        metrics::histogram!(self.metric.as_str(), self.start.elapsed());
    }
}

#[must_use = "leaf count should be `report()`ed"]
#[derive(Debug)]
pub(crate) struct LeafCountMetric(pub u64);

impl LeafCountMetric {
    const LEAF_COUNT: &'static str = "merkle_tree.leaf_count";

    fn describe() {
        metrics::describe_gauge!(
            Self::LEAF_COUNT,
            Unit::Count,
            "Current number of leaves in the tree"
        );
    }

    pub fn report(self) {
        metrics::gauge!(Self::LEAF_COUNT, self.0 as f64);
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct PatchNodeStats {
    /// Number of nodes.
    count: u64,
    /// Total serialized size of nodes (excluding key sizes).
    bytes: u64,
}

#[must_use = "patch metrics should be `report()`ed"]
#[derive(Debug)]
pub(crate) struct ApplyPatchMetrics {
    node_stats_by_nibble_count: [PatchNodeStats; Self::MAX_TRACKED_NIBBLE_COUNT + 1],
    copied_hashes: u64,
}

impl ApplyPatchMetrics {
    const MAX_TRACKED_NIBBLE_COUNT: usize = 6;

    const NODES: &'static str = "merkle_tree.apply_patch.nodes";
    const NODES_BY_NIBBLE_COUNT: &'static str = "merkle_tree.apply_patch.nodes_by_nibble_count";
    const NODE_BYTES: &'static str = "merkle_tree.apply_patch.node_bytes";
    const COPIED_HASHES: &'static str = "merkle_tree.apply_patch.copied_hashes";

    fn describe() {
        metrics::describe_histogram!(
            Self::NODES,
            Unit::Count,
            "Total number of nodes included into a RocksDB patch per block"
        );
        metrics::describe_histogram!(
            Self::NODES_BY_NIBBLE_COUNT,
            Unit::Count,
            "Number of nodes included into a RocksDB patch per block, grouped by \
             the key nibble count"
        );
        metrics::describe_histogram!(
            Self::NODE_BYTES,
            Unit::Bytes,
            "Total byte size of nodes included into a RocksDB patch per block, grouped by \
             the key nibble count"
        );
        metrics::describe_histogram!(
            Self::COPIED_HASHES,
            Unit::Count,
            "Number of hashes in child references copied from previous tree versions. \
             Allows to estimate the level of redundancy of the tree"
        );
    }

    pub fn new(copied_hashes: u64) -> Self {
        Self {
            node_stats_by_nibble_count: [PatchNodeStats::default();
                Self::MAX_TRACKED_NIBBLE_COUNT + 1],
            copied_hashes,
        }
    }

    pub fn update_node_bytes(&mut self, key_nibbles: &Nibbles, node_bytes: &[u8]) {
        let nibble_count = key_nibbles.nibble_count();
        let idx = nibble_count.min(Self::MAX_TRACKED_NIBBLE_COUNT);
        let stats = &mut self.node_stats_by_nibble_count[idx];
        stats.count += 1;
        stats.bytes += node_bytes.len() as u64;
    }

    pub fn report(self) {
        let total_node_count = self
            .node_stats_by_nibble_count
            .iter()
            .map(|stats| stats.count)
            .sum::<u64>();
        metrics::histogram!(Self::NODES, total_node_count as f64);

        let node_bytes = self.node_stats_by_nibble_count.iter().enumerate();
        for (nibble_count, stats) in node_bytes {
            let nibbles_tag = if nibble_count == Self::MAX_TRACKED_NIBBLE_COUNT {
                format!("{nibble_count}..")
            } else {
                nibble_count.to_string()
            };

            metrics::histogram!(
                Self::NODES_BY_NIBBLE_COUNT,
                stats.count as f64,
                "nibbles" => nibbles_tag.clone()
            );
            metrics::histogram!(
                Self::NODE_BYTES,
                stats.bytes as f64,
                "nibbles" => nibbles_tag
            );
        }

        metrics::histogram!(Self::COPIED_HASHES, self.copied_hashes as f64);
    }
}

#[derive(Debug)]
pub(crate) struct PruningStats {
    pub target_retained_version: u64,
    pub pruned_key_count: usize,
    pub deleted_stale_key_versions: ops::Range<u64>,
}

impl PruningStats {
    const TARGET_RETAINED_VERSION: &'static str = "merkle_tree.pruning.target_retained_version";
    const PRUNED_KEY_COUNT: &'static str = "merkle_tree.pruning.key_count";
    const DELETED_STALE_KEY_VERSIONS: &'static str =
        "merkle_tree.pruning.deleted_stale_key_versions";

    fn describe() {
        metrics::describe_gauge!(
            Self::TARGET_RETAINED_VERSION,
            Unit::Count,
            "Minimum Merkle tree version targeted after a single pruning iteration. The iteration \
             may not remove all stale keys to this version if there are too many"
        );
        metrics::describe_histogram!(
            Self::PRUNED_KEY_COUNT,
            Unit::Count,
            "Number of pruned node keys on a specific pruning iteration"
        );
        metrics::describe_gauge!(
            Self::DELETED_STALE_KEY_VERSIONS,
            Unit::Count,
            "Lower and upper boundaries on the new stale key versions deleted \
             during a pruning iteration. The lower boundary is inclusive, the upper one is exclusive"
        );
    }

    pub(crate) fn report(self) {
        metrics::gauge!(
            Self::TARGET_RETAINED_VERSION,
            self.target_retained_version as f64
        );
        metrics::histogram!(Self::PRUNED_KEY_COUNT, self.pruned_key_count as f64);
        metrics::gauge!(
            Self::DELETED_STALE_KEY_VERSIONS,
            self.deleted_stale_key_versions.start as f64,
            "bound" => "start"
        );
        metrics::gauge!(
            Self::DELETED_STALE_KEY_VERSIONS,
            self.deleted_stale_key_versions.end as f64,
            "bound" => "end"
        );
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum PruningTimings {
    LoadStaleKeys,
    ApplyPatch,
}

impl PruningTimings {
    const LOAD_STALE_KEYS: &'static str = "merkle_tree.pruning.load_stale_keys";
    const APPLY_PATCH: &'static str = "merkle_tree.pruning.apply_patch";

    fn describe() {
        metrics::describe_histogram!(
            Self::LOAD_STALE_KEYS,
            Unit::Seconds,
            "Time spent loading stale keys per pruning iteration"
        );
        metrics::describe_histogram!(
            Self::APPLY_PATCH,
            Unit::Seconds,
            "Time spent removing stale keys from RocksDB per pruning iteration"
        );
    }
}

impl Timing for PruningTimings {
    fn as_str(self) -> &'static str {
        match self {
            Self::LoadStaleKeys => Self::LOAD_STALE_KEYS,
            Self::ApplyPatch => Self::APPLY_PATCH,
        }
    }
}
