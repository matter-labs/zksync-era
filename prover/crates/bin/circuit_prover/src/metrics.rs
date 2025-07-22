use std::time::Duration;

use vise::{Gauge, Metrics};

/// Instrument prover binary lifecycle
#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_binary")]
pub struct ProverBinaryMetrics {
    /// How long does it take for prover to load data before it can produce proofs?
    pub startup_time: Gauge<Duration>,
    /// How long did the prover binary run for?
    pub run_time: Gauge<Duration>,
    /// How long does it take prover to gracefully shutdown?
    pub shutdown_time: Gauge<Duration>,
}

#[vise::register]
pub static PROVER_BINARY_METRICS: vise::Global<ProverBinaryMetrics> = vise::Global::new();
