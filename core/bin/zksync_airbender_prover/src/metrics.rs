//! Metrics for the TEE Prover.

use std::time::Duration;

use vise::{Buckets, Counter, Gauge, Histogram, Metrics, Unit};

#[derive(Debug, Metrics)]
#[metrics(prefix = "airbender_prover")]
pub(crate) struct AirbenderProverMetrics {
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub job_waiting_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub proof_generation_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub proof_submitting_time: Histogram<Duration>,
    pub network_errors_counter: Counter<u64>,
    pub last_batch_number_processed: Gauge<u64>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<AirbenderProverMetrics> = vise::Global::new();
