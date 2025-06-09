use vise::{Buckets, Histogram, LabeledFamily, Metrics, Unit};

const OP_SIZE_BUCKETS: Buckets = Buckets::exponential(256.0..=256.0 * 1_024.0 * 1_204.0, 4.0);

#[derive(Debug, Metrics)]
#[metrics(prefix = "jemalloc_operation")]
pub(super) struct JemallocOpMetrics {
    /// Number of bytes allocated when performing a certain operation.
    #[metrics(labels = ["op"], buckets = OP_SIZE_BUCKETS, unit = Unit::Bytes)]
    allocated: LabeledFamily<&'static str, Histogram<u64>>,
    /// Number of bytes deallocated when performing a certain operation.
    #[metrics(labels = ["op"], buckets = OP_SIZE_BUCKETS, unit = Unit::Bytes)]
    deallocated: LabeledFamily<&'static str, Histogram<u64>>,
    /// Ratio of deallocated to allocated bytes. >1.0 means that more memory was deallocated than allocated during the op
    /// (this is normal).
    #[metrics(labels = ["op"], buckets = Buckets::linear(0.5..=1.5, 0.05))]
    alloc_ratio: LabeledFamily<&'static str, Histogram<f64>>,
}

impl JemallocOpMetrics {
    pub(super) fn observe_op_stats(&self, op: &'static str, allocated: u64, deallocated: u64) {
        const LOG_THRESHOLD: u64 = 1_024; // FIXME: 128 << 20; // 128 MB

        self.allocated[&op].observe(allocated);
        self.deallocated[&op].observe(deallocated);
        self.alloc_ratio[&op].observe(allocated as f64 / deallocated as f64);

        if allocated >= LOG_THRESHOLD {
            tracing::debug!(
                op,
                allocated,
                deallocated,
                "Operation {op} resulted in large (de)allocations"
            );
        }
    }
}

#[vise::register]
pub(crate) static METRICS: vise::Global<JemallocOpMetrics> = vise::Global::new();
