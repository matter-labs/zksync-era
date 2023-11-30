use std::time::Duration;
use vise::{Buckets, Counter, Histogram, LabeledFamily, Metrics};

const PROVER_LATENCY_BUCKETS: Buckets = Buckets::values(&[
    1.0, 10.0, 20.0, 40.0, 60.0, 120.0, 240.0, 360.0, 600.0, 1800.0, 3600.0,
]);

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover")]
pub(crate) struct ProverMetrics {
    #[metrics(buckets = PROVER_LATENCY_BUCKETS, labels = ["circuit_type"])]
    pub proof_generation_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(buckets = PROVER_LATENCY_BUCKETS, labels = ["circuit_type"])]
    pub circuit_synthesis_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(buckets = PROVER_LATENCY_BUCKETS, labels = ["circuit_type"])]
    pub assembly_finalize_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(buckets = PROVER_LATENCY_BUCKETS, labels = ["circuit_type"])]
    pub assembly_encoding_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(buckets = PROVER_LATENCY_BUCKETS, labels = ["circuit_type"])]
    pub assembly_decoding_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(buckets = PROVER_LATENCY_BUCKETS, labels = ["circuit_type"])]
    pub assembly_transferring_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(buckets = PROVER_LATENCY_BUCKETS, labels = ["circuit_type"])]
    pub setup_load_time: LabeledFamily<String, Histogram<Duration>>,
    #[metrics(labels = ["circuit_type"])]
    pub setup_loading_cache_miss: LabeledFamily<String, Counter>,
    #[metrics(buckets = PROVER_LATENCY_BUCKETS)]
    pub prover_wait_idle_time: Histogram<Duration>,
    #[metrics(buckets = PROVER_LATENCY_BUCKETS)]
    pub setup_load_wait_idle_time: Histogram<Duration>,
    #[metrics(buckets = PROVER_LATENCY_BUCKETS)]
    pub scheduler_wait_idle_time: Histogram<Duration>,
    #[metrics(buckets = PROVER_LATENCY_BUCKETS)]
    pub download_time: Histogram<Duration>,
    #[metrics(buckets = PROVER_LATENCY_BUCKETS, labels = ["queue_capacity"])]
    pub queue_free_slots: LabeledFamily<&'static str, Histogram>,
}

#[vise::register]
pub(crate) static PROVER_METRICS: vise::Global<ProverMetrics> = vise::Global::new();
