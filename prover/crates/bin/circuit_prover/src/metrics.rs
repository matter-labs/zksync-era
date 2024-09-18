use std::time::Duration;

use vise::{Buckets, Histogram, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_binary")]
pub struct ProverBinaryMetrics {
    /// How long does it take for prover to load data before it can produce proofs?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub start_up: Histogram<Duration>,
    /// How long has the prover been running?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub run_time: Histogram<Duration>,
}

#[vise::register]
pub static PROVER_BINARY_METRICS: vise::Global<ProverBinaryMetrics> = vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "witness_vector_generator")]
pub struct WitnessVectorGeneratorMetrics {
    /// How long does witness vector generator waits before a job is available?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub job_wait_time: Histogram<Duration>,
    /// How long does it take to load object store artifacts for a witness vector job?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub artifact_download_time: Histogram<Duration>,
    /// How long does the crypto witness generation primitive take?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub crypto_primitive_time: Histogram<Duration>,
    /// How long does it take for a job to be executed, from the moment it's loaded?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub execution_time: Histogram<Duration>,
    /// How long does it take to send a job to prover?
    /// This is relevant because prover queue can apply back-pressure.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub send_time: Histogram<Duration>,
    /// How long does it take for a job to be considered finished, from the moment it's been loaded?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub job_finished_time: Histogram<Duration>,
}

#[vise::register]
pub static WITNESS_VECTOR_GENERATOR_METRICS: vise::Global<WitnessVectorGeneratorMetrics> =
    vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "circuit_prover")]
pub struct CircuitProverMetrics {
    /// How long does circuit prover wait before a job is available?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub job_wait_time: Histogram<Duration>,
    /// How long does the crypto primitives (proof generation & verification) take?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub crypto_primitives_time: Histogram<Duration>,
    /// How long does proof generation (crypto primitive) take?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub generate_proof_time: Histogram<Duration>,
    /// How long does verify proof (crypto primitive) take?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub verify_proof_time: Histogram<Duration>,
    /// How long does it take for a job to be executed, from the moment it's loaded?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub execution_time: Histogram<Duration>,
    /// How long does it take to upload proof to object store?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub artifact_upload_time: Histogram<Duration>,
    /// How long does it take to save a job?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub save_time: Histogram<Duration>,
    /// How long does it take for a job to be considered finished, from the moment it's been loaded?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub job_finished_time: Histogram<Duration>,
    /// How long does it take a job to go from witness generation to having the proof saved?
    #[metrics(buckets = Buckets::LATENCIES)]
    pub full_proving_time: Histogram<Duration>,
}

#[vise::register]
pub static CIRCUIT_PROVER_METRICS: vise::Global<CircuitProverMetrics> = vise::Global::new();
