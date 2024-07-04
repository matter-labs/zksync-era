//! Metrics for the TEE Prover.

use std::time::Duration;

use vise::{Buckets, Gauge, Histogram, Metrics, Unit};

#[derive(Debug, Metrics)]
#[metrics(prefix = "tee_prover")]
pub(crate) struct TeeProverMetrics {
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub job_waiting_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub proof_generation_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub proof_submitting_time: Histogram<Duration>,
    pub network_errors_counter: Gauge<u64>,
    pub block_number_processed: Gauge<u64>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<TeeProverMetrics> = vise::Global::new();
