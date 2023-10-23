//! Metrics for the synchronization layer of external node.

use vise::{Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics};

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum FetchStage {
    GetMiniblockRange,
    GetBlockDetails,
    SyncL2Block,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum L1BatchStage {
    Open,
    Committed,
    Proven,
    Executed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "method", rename_all = "snake_case")]
pub(super) enum CachedMethod {
    SyncL2Block,
}

/// Metrics for the fetcher.
#[derive(Debug, Metrics)]
#[metrics(prefix = "external_node_fetcher")]
pub(super) struct FetcherMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub requests: Family<FetchStage, Histogram<Duration>>,
    pub l1_batch: Family<L1BatchStage, Gauge<u64>>,
    pub miniblock: Gauge<u64>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub fetch_next_miniblock: Histogram<Duration>,

    // Cache-related metrics.
    pub cache_total: Family<CachedMethod, Counter>,
    pub cache_hit: Family<CachedMethod, Counter>,
    pub cache_errors: Counter,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub cache_populate: Histogram<Duration>,
}

#[vise::register]
pub(super) static FETCHER_METRICS: vise::Global<FetcherMetrics> = vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "external_node_action_queue")]
pub(super) struct ActionQueueMetrics {
    pub action_queue_size: Gauge<usize>,
}

#[vise::register]
pub(super) static QUEUE_METRICS: vise::Global<ActionQueueMetrics> = vise::Global::new();
