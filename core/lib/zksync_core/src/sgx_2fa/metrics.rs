//! Sgx2fa metrics.

use std::time::Duration;

use vise::{Buckets, Gauge, Histogram, Metrics, Unit};

#[derive(Debug, Metrics)]
#[metrics(prefix = "sgx_2fa")]
pub(crate) struct Sgx2faMetrics {
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub process_batch_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub upload_input_time: Histogram<Duration>,
    pub block_number_processed: Gauge,
}

#[vise::register]
pub(super) static METRICS: vise::Global<Sgx2faMetrics> = vise::Global::new();
