use std::time::Duration;

use vise::{Buckets, Histogram, Metrics};

/// Metrics for witness vector generator execution
#[derive(Debug, Metrics)]
#[metrics(prefix = "witness_vector_generator")]
pub struct WitnessVectorGeneratorMetrics {
    /// How long does it take to load witness vector inputs?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub pick_time: Histogram<Duration>,
    /// How long does it take to synthesize witness vector?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub synthesize_time: Histogram<Duration>,
    /// How long does it take to send witness vectors to gpu prover?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub transfer_time: Histogram<Duration>,
    /// How long does it take to save witness vector failure?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub save_time: Histogram<Duration>,
}

#[vise::register]
pub static WITNESS_VECTOR_GENERATOR_METRICS: vise::Global<WitnessVectorGeneratorMetrics> =
    vise::Global::new();

/// Metrics for GPU circuit prover execution
#[derive(Debug, Metrics)]
#[metrics(prefix = "circuit_prover")]
pub struct CircuitProverMetrics {
    /// How long does it take to load prover inputs?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub load_time: Histogram<Duration>,
    /// How long does it take to prove & verify?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub prove_and_verify_time: Histogram<Duration>,
    /// How long does it take to save prover results?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub save_time: Histogram<Duration>,
    /// How long does it take finish a prover job from witness vector to circuit prover?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub full_time: Histogram<Duration>,
}

#[vise::register]
pub static CIRCUIT_PROVER_METRICS: vise::Global<CircuitProverMetrics> = vise::Global::new();
