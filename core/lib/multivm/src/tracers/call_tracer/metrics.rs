use vise::{Buckets, Histogram, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "vm_call_tracer")]
pub struct CallMetrics {
    /// Maximum call stack depth during the execution of the transaction.
    #[metrics(buckets = Buckets::exponential(1.0..=4096.0, 2.0))]
    pub call_stack_depth: Histogram<usize>,
    /// Maximum number of near calls during the execution of the transaction.
    #[metrics(buckets = Buckets::exponential(1.0..=4096.0, 2.0))]
    pub max_near_calls: Histogram<usize>,
    /// Call input / output bytes dropped because the transaction exceeded the trace data budget.
    /// Only observed for transactions that actually exceeded it.
    #[metrics(buckets = Buckets::exponential(1_024.0..=4_294_967_296.0, 4.0))]
    pub dropped_data_bytes: Histogram<usize>,
}

#[vise::register]
pub static CALL_METRICS: vise::Global<CallMetrics> = vise::Global::new();
