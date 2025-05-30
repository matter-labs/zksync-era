use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};
use zksync_multivm::interface::{storage::StorageViewStats, VmMemoryMetrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "type", rename_all = "snake_case")]
enum SizeType {
    Inner,
    History,
}

const MEMORY_SIZE_BUCKETS: Buckets = Buckets::values(&[
    1_000.0,
    10_000.0,
    100_000.0,
    500_000.0,
    1_000_000.0,
    5_000_000.0,
    10_000_000.0,
    50_000_000.0,
    100_000_000.0,
    500_000_000.0,
    1_000_000_000.0,
]);

#[derive(Debug, Metrics)]
#[metrics(prefix = "runtime_context_memory")]
struct RuntimeContextMemoryMetrics {
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    event_sink_size: Family<SizeType, Histogram<usize>>,
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    memory_size: Family<SizeType, Histogram<usize>>,
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    decommitter_size: Family<SizeType, Histogram<usize>>,
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    storage_size: Family<SizeType, Histogram<usize>>,
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    storage_view_cache_size: Histogram<usize>,
    #[metrics(buckets = MEMORY_SIZE_BUCKETS)]
    full: Histogram<usize>,
}

#[vise::register]
static MEMORY_METRICS: vise::Global<RuntimeContextMemoryMetrics> = vise::Global::new();

pub(super) fn report_vm_memory_metrics(
    memory_metrics: &VmMemoryMetrics,
    storage_stats: &StorageViewStats,
) {
    MEMORY_METRICS.event_sink_size[&SizeType::Inner].observe(memory_metrics.event_sink_inner);
    MEMORY_METRICS.event_sink_size[&SizeType::History].observe(memory_metrics.event_sink_history);
    MEMORY_METRICS.memory_size[&SizeType::Inner].observe(memory_metrics.memory_inner);
    MEMORY_METRICS.memory_size[&SizeType::History].observe(memory_metrics.memory_history);
    MEMORY_METRICS.decommitter_size[&SizeType::Inner]
        .observe(memory_metrics.decommittment_processor_inner);
    MEMORY_METRICS.decommitter_size[&SizeType::History]
        .observe(memory_metrics.decommittment_processor_history);
    MEMORY_METRICS.storage_size[&SizeType::Inner].observe(memory_metrics.storage_inner);
    MEMORY_METRICS.storage_size[&SizeType::History].observe(memory_metrics.storage_history);

    MEMORY_METRICS
        .storage_view_cache_size
        .observe(storage_stats.cache_size);
    MEMORY_METRICS
        .full
        .observe(memory_metrics.full_size() + storage_stats.cache_size);
}
