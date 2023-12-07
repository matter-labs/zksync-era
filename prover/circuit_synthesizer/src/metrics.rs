use std::time::Duration;

use vise::{Buckets, Histogram, LabeledFamily, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_circuit_synthesizer")]
pub(crate) struct CircuitSynthesizerMetrics {
    #[metrics(buckets = Buckets::LATENCIES, labels = ["blob_size_in_gb"])]
    pub blob_sending_time: LabeledFamily<u64, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = ["circuit_type"])]
    pub synthesize: LabeledFamily<&'static str, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<CircuitSynthesizerMetrics> = vise::Global::new();
