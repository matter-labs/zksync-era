// use std::time::Duration;
//
// use vise::{
//     Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Histogram, LabeledFamily, Metrics,
// };
//
// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
// pub(crate) struct CircuitLabels {
//     pub circuit_type: u8,
//     pub layer: Layer,
// }
//
// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
// #[metrics(rename_all = "snake_case")]
// pub(crate) enum Layer {
//     Recursive,
//     Base,
// }
//
// #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
// #[metrics(label = "reason", rename_all = "snake_case")]
// #[allow(dead_code)]
// pub(crate) enum KillingReason {
//     /// Prover was found with Dead status in the database
//     Dead,
//     /// Prover was not found in the database
//     Absent,
// }
//
// #[derive(Debug, Metrics)]
// #[metrics(prefix = "prover_fri_prover")]
// pub(crate) struct ProverFriMetrics {
//     #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
//     pub gpu_setup_data_load_time: LabeledFamily<String, Histogram<Duration>>,
//     #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
//     pub gpu_proof_generation_time: LabeledFamily<String, Histogram<Duration>>,
//     #[metrics(buckets = Buckets::LATENCIES)]
//     pub gpu_total_proving_time: Histogram<Duration>,
//     #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
//     pub setup_data_load_time: LabeledFamily<String, Histogram<Duration>>,
//     #[metrics(buckets = Buckets::LATENCIES)]
//     pub proof_generation_time: Family<CircuitLabels, Histogram<Duration>>,
//     #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
//     pub proof_verification_time: LabeledFamily<String, Histogram<Duration>>,
//     #[metrics(buckets = Buckets::LATENCIES)]
//     pub cpu_total_proving_time: Histogram<Duration>,
//     #[metrics(buckets = Buckets::LATENCIES, labels = ["blob_size_in_gb"])]
//     pub witness_vector_blob_time: LabeledFamily<u64, Histogram<Duration>>,
//     #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
//     pub blob_save_time: LabeledFamily<String, Histogram<Duration>>,
//     pub zombie_prover_instances_count: Family<KillingReason, Counter>,
// }
//
// #[vise::register]
// pub(crate) static METRICS: vise::Global<ProverFriMetrics> = vise::Global::new();

use std::time::Duration;

use vise::{Buckets, Histogram, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_binary")]
pub struct ProverBinaryMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub start_up: Histogram<Duration>,
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
