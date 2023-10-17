//! Metrics for the Merkle tree API.

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics, Unit};

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "method", rename_all = "snake_case")]
pub(super) enum MerkleTreeApiMethod {
    Info,
    GetProofs,
}

/// Metrics for Merkle tree API.
#[derive(Debug, Metrics)]
#[metrics(prefix = "server_merkle_tree_api")]
pub(super) struct MerkleTreeApiMetrics {
    /// Server latency of the Merkle tree API methods.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub latency: Family<MerkleTreeApiMethod, Histogram<Duration>>,
}

#[vise::register]
pub(super) static API_METRICS: vise::Global<MerkleTreeApiMetrics> = vise::Global::new();
