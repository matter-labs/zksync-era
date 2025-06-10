use vise::{Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Metrics, Unit};

use crate::json::{ArenaStatsMap, GeneralStats, OperationStats};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(rename_all = "snake_case", label = "size")]
enum AllocationSize {
    Small,
    Large,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
enum Op {
    Malloc,
    Dalloc,
    Fill,
    Flush,
}

impl Op {
    const ALL: [Self; 4] = [Self::Malloc, Self::Dalloc, Self::Fill, Self::Flush];

    fn extract(self, stats: &OperationStats) -> u64 {
        match self {
            Self::Malloc => stats.nmalloc,
            Self::Dalloc => stats.ndalloc,
            Self::Fill => stats.nfills,
            Self::Flush => stats.nflushes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
struct AllocationOpLabels {
    size: AllocationSize,
    op: Op,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "jemalloc")]
pub(crate) struct JemallocMetrics {
    /// Number of bytes currently allocated by Jemalloc.
    #[metrics(unit = Unit::Bytes)]
    total_allocated: Gauge<u64>,
    /// Number of active bytes reported by Jemalloc.
    #[metrics(unit = Unit::Bytes)]
    total_active: Gauge<u64>,
    /// Number of metadata bytes reported by Jemalloc.
    #[metrics(unit = Unit::Bytes)]
    total_metadata: Gauge<u64>,
    /// Number of resident bytes reported by Jemalloc.
    #[metrics(unit = Unit::Bytes)]
    total_resident: Gauge<u64>,
    /// Number of bytes in the mapped memory reported by Jemalloc.
    #[metrics(unit = Unit::Bytes)]
    total_mapped: Gauge<u64>,
    /// Number of application threads that ever allocated using Jemalloc.
    thread_count: Gauge<u64>,

    /// Allocations grouped by the allocation size.
    #[metrics(unit = Unit::Bytes)]
    arena_allocated: Family<AllocationSize, Gauge<u64>>,
    /// Number of operations grouped by the allocation size.
    arena_operations: Family<AllocationOpLabels, Counter>,
}

impl JemallocMetrics {
    pub(crate) fn observe_general_stats(&self, stats: &GeneralStats) {
        tracing::debug!(?stats, "Retrieved general Jemalloc stats");
        self.total_allocated.set(stats.allocated);
        self.total_active.set(stats.active);
        self.total_metadata.set(stats.metadata);
        self.total_resident.set(stats.resident);
        self.total_mapped.set(stats.mapped);
    }

    pub(crate) fn observe_arena_stats(&self, stats: &ArenaStatsMap) {
        let stats = &stats.merged;
        self.thread_count.set(stats.nthreads);
        self.arena_allocated[&AllocationSize::Small].set(stats.small.allocated);
        self.observe_ops(AllocationSize::Small, &stats.small);
        self.arena_allocated[&AllocationSize::Large].set(stats.large.allocated);
        self.observe_ops(AllocationSize::Large, &stats.large);
    }

    fn observe_ops(&self, size: AllocationSize, stats: &OperationStats) {
        tracing::debug!(
            ?size,
            ?stats,
            "Retrieved Jemalloc stats for {size:?} allocations"
        );
        for op in Op::ALL {
            let current_value = op.extract(stats);
            let counter = &self.arena_operations[&AllocationOpLabels { size, op }];
            let prev_value = counter.get();
            counter.inc_by(current_value.saturating_sub(prev_value));
        }
    }
}

#[vise::register]
pub(crate) static METRICS: vise::Global<JemallocMetrics> = vise::Global::new();
