use std::time::Duration;

use vise::{Gauge, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri_proof_fri_compressor")]
pub(crate) struct ProofFriCompressorMetrics {
    /// How long does it take for compressor to load data before it can produce proofs?
    pub startup_time: Gauge<Duration>,
    /// How long did the compressor binary run for?
    pub run_time: Gauge<Duration>,
    /// How long does it take compressor to gracefully shutdown?
    pub shutdown_time: Gauge<Duration>,
}

#[vise::register]
pub(crate) static PROOF_FRI_COMPRESSOR_INSTANCE_METRICS: vise::Global<ProofFriCompressorMetrics> =
    vise::Global::new();
