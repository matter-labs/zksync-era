use std::time::Duration;
use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, LabeledFamily,
    Metrics,
};

// FIXME: maybe we can use basic enum from zksync_types instead of this one?
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "aggregation_round", rename_all = "snake_case")]
pub(crate) enum AggregationRound {
    BasicCircuits,
    LeafAggregation,
    NodeAggregation,
    Scheduler,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri_witness_generator")]
pub(crate) struct WitnessGeneratorMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_fetch_time: Family<AggregationRound, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub prepare_job_time: Family<AggregationRound, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub witness_generation_time: Family<AggregationRound, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_save_time: Family<AggregationRound, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static WITNESS_GENERATOR_METRICS: vise::Global<WitnessGeneratorMetrics> =
    vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_witness_generator_fri")]
pub(crate) struct ServerWitnessGeneratorMetrics {
    pub sampled_blocks: Counter,
    pub skipped_blocks: Counter,
}

#[vise::register]
pub(crate) static SERVER_WITNESS_GENERATOR_METRICS: vise::Global<ServerWitnessGeneratorMetrics> =
    vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "server")]
pub(crate) struct ServerMetrics {
    #[metrics(labels = "stage")]
    pub latency: LabeledFamily<&'static str, Gauge<Duration>>,
}

#[vise::register]
pub(crate) static SERVER_METRICS: vise::Global<ServerMetrics> = vise::Global::new();
