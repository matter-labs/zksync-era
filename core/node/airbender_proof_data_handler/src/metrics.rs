use std::time::Duration;

use vise::{Gauge, Histogram, Metrics, Unit};

#[derive(Debug, Metrics)]
pub(super) struct AirbenderProofDataHandlerMetrics {
    #[metrics(buckets = vise::Buckets::LATENCIES, unit = Unit::Seconds)]
    pub airbender_proof_roundtrip_time: Histogram<Duration>,
    /// Number of batches that are ready to be proven by airbender provers.
    pub batches_ready_for_proving: Gauge<i64>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<AirbenderProofDataHandlerMetrics> = vise::Global::new();
