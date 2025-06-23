use vise::{Buckets, Counter, Histogram, LabeledFamily, Metrics, Unit};

use super::AllocationStats;

const OP_SIZE_BUCKETS: Buckets = Buckets::exponential(4_096.0..=(1 << 30) as f64, 4.0);

#[derive(Debug, Metrics)]
#[metrics(prefix = "jemalloc_operation")]
pub(super) struct JemallocOpMetrics {
    /// Number of bytes allocated when performing a certain operation.
    #[metrics(labels = ["op"], buckets = OP_SIZE_BUCKETS, unit = Unit::Bytes)]
    allocated: LabeledFamily<String, Histogram<u64>>,
    /// Number of bytes deallocated when performing a certain operation.
    #[metrics(labels = ["op"], buckets = OP_SIZE_BUCKETS, unit = Unit::Bytes)]
    deallocated: LabeledFamily<String, Histogram<u64>>,
    /// Ratio of deallocated to allocated bytes. >1.0 means that more memory was deallocated than allocated during the op
    /// (this is normal).
    #[metrics(labels = ["op"], buckets = Buckets::linear(0.5..=1.5, 0.05))]
    alloc_ratio: LabeledFamily<String, Histogram<f64>>,
}

impl JemallocOpMetrics {
    pub(super) fn observe_op_stats(&self, op: &str, stats: AllocationStats) {
        const LOG_THRESHOLD: u64 = 64 << 20; // 64 MB

        self.allocated[op].observe(stats.allocated);
        self.deallocated[op].observe(stats.deallocated);
        self.alloc_ratio[op].observe(stats.allocated as f64 / stats.deallocated as f64);

        if stats.allocated >= LOG_THRESHOLD {
            tracing::debug!(
                op,
                allocated = stats.allocated,
                deallocated = stats.deallocated,
                "Operation {op} resulted in large (de)allocations"
            );
        }
    }
}

#[vise::register]
pub(crate) static OP_METRICS: vise::Global<JemallocOpMetrics> = vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "jemalloc_task")]
pub(super) struct JemallocTaskMetrics {
    #[metrics(labels = ["task"], unit = Unit::Bytes)]
    allocated: LabeledFamily<String, Counter>,
    #[metrics(labels = ["task"], unit = Unit::Bytes)]
    deallocated: LabeledFamily<String, Counter>,
}

impl JemallocTaskMetrics {
    pub(super) fn observe_task_increments(&self, task: &str, stats: AllocationStats) {
        self.allocated[task].inc_by(stats.allocated);
        self.deallocated[task].inc_by(stats.deallocated);
    }
}

#[vise::register]
pub(crate) static TASK_METRICS: vise::Global<JemallocTaskMetrics> = vise::Global::new();
