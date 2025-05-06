use std::time::Duration;

use vise::{Gauge, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "proof_fri_compressor_instance")]
pub(crate) struct ProofFriCompressorMetrics {
    /// How long does it take for prover to load data before it can produce proofs?
    pub startup_time: Gauge<Duration>,
    /// How long did the prover binary run for?
    pub run_time: Gauge<Duration>,
    /// How long does it take prover to gracefully shutdown?
    pub shutdown_time: Gauge<Duration>,
}

#[vise::register]
pub(crate) static PROOF_FRI_COMPRESSOR_INSTANCE_METRICS: vise::Global<ProofFriCompressorMetrics> =
    vise::Global::new();
