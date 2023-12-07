use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, LabeledFamily, Metrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct CircuitLabels {
    pub circuit_type: u8,
    pub layer: Layer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub(crate) enum Layer {
    Recursive,
    Base,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri_prover")]
pub(crate) struct ProverFriMetrics {
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub gpu_setup_data_load_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub gpu_proof_generation_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub gpu_total_proving_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub setup_data_load_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub proof_generation_time: Family<CircuitLabels, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub proof_verification_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub cpu_total_proving_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["blob_size_in_gb"])]
    pub witness_vector_blob_time: LabeledFamily<u64, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub gpu_assembly_generation_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub blob_save_time: LabeledFamily<String, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<ProverFriMetrics> = vise::Global::new();
