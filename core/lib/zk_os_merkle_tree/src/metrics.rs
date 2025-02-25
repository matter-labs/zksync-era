//! Merkle tree metrics.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics, Unit};

const NODE_COUNT_BUCKETS: Buckets = Buckets::values(&[
    100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0, 50_000.0, 100_000.0,
]);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(rename_all = "snake_case", label = "stage")]
pub(crate) enum LoadStage {
    Total,
    KeyLookup,
    TreeNodes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(rename_all = "snake_case", label = "stage")]
pub(crate) enum BatchProofStage {
    Total,
    Hashing,
    Traversal,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "zk_os_merkle_tree")]
pub(crate) struct MerkleTreeMetrics {
    /// Current number of leaves in the tree.
    pub leaf_count: Gauge<u64>,

    // Latencies of different operations
    /// Time spent loading tree nodes from DB per batch.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub load_nodes_latency: Family<LoadStage, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub batch_proof_latency: Family<BatchProofStage, Histogram<Duration>>,
    /// Time spent traversing the tree and creating new nodes per batch.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub extend_patch_latency: Histogram<Duration>,
    /// Time spent finalizing a batch (mainly hash computations).
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub finalize_patch_latency: Histogram<Duration>,
    /// Time spent applying a batch to the database.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub apply_patch_latency: Histogram<Duration>,

    /// Number of updated leaves in a batch.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    pub batch_updates_count: Histogram<usize>,
    /// Number of newly inserted leaves in a batch.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    pub batch_inserts_count: Histogram<usize>,
    /// Number of keys read in a batch. Only set in the proof generation mode.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    pub batch_reads_count: Histogram<usize>,
    /// Number of missing keys read in a batch. Only set in the proof generation mode.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    pub batch_missing_reads_count: Histogram<usize>,
    /// Number of leaves loaded for a batch update.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    pub loaded_leaves: Histogram<usize>,
    /// Number of internal nodes loaded for a batch update.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    pub loaded_internal_nodes: Histogram<usize>,

    /// Number of hashes included in a generated proof.
    #[metrics(buckets = NODE_COUNT_BUCKETS)]
    pub proof_hashes_count: Histogram<usize>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<MerkleTreeMetrics> = vise::Global::new();
