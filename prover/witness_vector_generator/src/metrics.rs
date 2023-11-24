use std::time::Duration;
use vise::{Buckets, Histogram, LabeledFamily, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri.witness_vector_generator")]
pub(crate) struct WitnessVectorGeneratorMetrics {
    #[metrics(buckets = Buckets::LATENCIES, labels = "method")]
    pub gpu_witness_vector_generation_time: LabeledFamily<&'static str, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, labels = "method")]
    pub blob_sending_time: LabeledFamily<&'static str, Histogram<Duration>>,
}

pub(crate) static WITNESS_VECTOR_GENERATOR_METRICS: vise::Global<WitnessVectorGeneratorMetrics> =
    vise::Global::new();
