use vise::{Buckets, Histogram, Metrics};

#[derive(Debug, Metrics)]
pub struct CallMetrics {
    #[metrics(buckets = Buckets::linear(1.0..=64.0, 1.0))]
    pub call_depth: Histogram<usize>,
    #[metrics(buckets = Buckets::linear(1.0..=64.0, 1.0))]
    pub max_near_calls: Histogram<usize>,
}

#[vise::register]
pub static CALL_METRICS: vise::Global<CallMetrics> = vise::Global::new();
