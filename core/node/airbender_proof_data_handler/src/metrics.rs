use std::time::Duration;

use vise::{Histogram, Metrics, Unit};

#[derive(Debug, Metrics)]
pub(super) struct AirbenderProofDataHandlerMetrics {
    #[metrics(buckets = vise::Buckets::LATENCIES, unit = Unit::Seconds)]
    pub airbender_proof_roundtrip_time: Histogram<Duration>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<AirbenderProofDataHandlerMetrics> = vise::Global::new();
