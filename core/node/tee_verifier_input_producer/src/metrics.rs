//! Metrics

use std::time::Duration;

use vise::{Buckets, Gauge, Histogram, Metrics, Unit};

#[derive(Debug, Metrics)]
#[metrics(prefix = "tee_verifier_input_producer")]
pub(crate) struct TeeVerifierInputProducerMetrics {
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub process_batch_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub upload_input_time: Histogram<Duration>,
    pub block_number_processed: Gauge<u64>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<TeeVerifierInputProducerMetrics> = vise::Global::new();
