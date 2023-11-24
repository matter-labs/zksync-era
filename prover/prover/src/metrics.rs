use std::time::Duration;
use vise::{Buckets, Counter, Histogram, LabeledFamily, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "server.prover")]
pub(crate) struct ProverMetrics {
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub proof_generation_time: LabeledFamily<&'static str, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub circuit_synthesis_time: LabeledFamily<&'static str, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub assembly_finalize_time: LabeledFamily<&'static str, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub assembly_encoding_time: LabeledFamily<&'static str, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub assembly_decoding_time: LabeledFamily<&'static str, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub assembly_transferring_time: LabeledFamily<&'static str, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub setup_load_time: LabeledFamily<&'static str, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub setup_loading_cache_miss: LabeledFamily<&'static str, Counter>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub prover_wait_idle_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub setup_load_wait_idle_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub scheduler_wait_idle_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub download_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["queue_capacity"])]
    pub queue_free_slots: LabeledFamily<&'static str, Histogram>,
}

#[vise::register]
pub(crate) static PROVER_METRICS: vise::Global<ProverMetrics> = vise::Global::new();
