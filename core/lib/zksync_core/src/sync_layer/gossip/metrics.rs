//! Metrics for gossip-powered syncing.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "kind", rename_all = "snake_case")]
pub(super) enum BlockResponseKind {
    Persisted,
    InMemory,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "external_node_gossip_fetcher")]
pub(super) struct GossipFetcherMetrics {
    /// Number of currently buffered unexecuted blocks.
    pub buffer_size: Gauge<usize>,
    /// Latency of a `get_block` call.
    #[metrics(unit = Unit::Seconds, buckets = Buckets::LATENCIES)]
    pub get_block_latency: Family<BlockResponseKind, Histogram<Duration>>,
    /// Latency of putting a block into the buffered storage. This may include the time to queue
    /// block actions, but does not include block execution.
    #[metrics(unit = Unit::Seconds, buckets = Buckets::LATENCIES)]
    pub buffer_block_latency: Histogram<Duration>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<GossipFetcherMetrics> = vise::Global::new();
