//! BasicWitnessInputProducer metrics.

use std::time::Duration;

use vise::{Buckets, Histogram, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "basic_witness_input_producer")]
pub(crate) struct BasicWitnessInputProducerMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub process_batch_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub upload_input_time: Histogram<Duration>,
}

#[vise::register]
pub(crate) static BASIC_WITNESS_INPUT_PRODUCER_METRICS: vise::Global<
    BasicWitnessInputProducerMetrics,
> = vise::Global::new();
