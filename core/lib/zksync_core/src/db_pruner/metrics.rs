use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics, Unit};
use zksync_dal::pruning_dal::PruneType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "prune_type", rename_all = "snake_case")]
pub(crate) enum MetricPruneType {
    Soft,
    Hard,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "db_pruner")]
pub(crate) struct DbPrunerMetrics {
    /// Total latency of pruning chunk of l1 batches.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub pruning_chunk_duration: Family<MetricPruneType, Histogram<Duration>>,

    /// Number of not-pruned l1 batches
    pub not_pruned_l1_batches_count: Gauge<u64>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<DbPrunerMetrics> = vise::Global::new();
