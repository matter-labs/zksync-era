use std::time::Duration;

use vise::{Buckets, Histogram, Metrics, Unit};

/// Buckets for proof roundtrip times, ranging from 1 minute to 4 hours.
const PROOF_ROUNDTRIP_BUCKETS: Buckets = Buckets::values(&[
    60.0, 120.0, 300.0, 600.0, 900.0, 1200.0, 1800.0, 3600.0, 7200.0, 14400.0,
]);

#[derive(Debug, Metrics)]
pub(super) struct AirbenderProofDataHandlerMetrics {
    #[metrics(buckets = PROOF_ROUNDTRIP_BUCKETS, unit = Unit::Seconds)]
    pub airbender_proof_roundtrip_time: Histogram<Duration>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<AirbenderProofDataHandlerMetrics> = vise::Global::new();
