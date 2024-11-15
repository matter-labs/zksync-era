use std::time::Duration;

use vise::{Buckets, Histogram, Metrics};

/// Instrument prover binary lifecycle
#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_binary")]
pub struct ProverBinaryMetrics {
    /// How long does it take for prover to load data before it can produce proofs?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub startup_time: Histogram<Duration>,
    /// How long did the prover binary run for?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub run_time: Histogram<Duration>,
    /// How long does it take prover to gracefully shutdown?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub shutdown_time: Histogram<Duration>,
}

#[vise::register]
pub static PROVER_BINARY_METRICS: vise::Global<ProverBinaryMetrics> = vise::Global::new();
