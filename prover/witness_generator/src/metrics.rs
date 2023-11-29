use std::time::Duration;
use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, LabeledFamily,
    Metrics,
};
use zksync_types::proofs::AggregationRound;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", format = "wit_gen_{}")]
pub(crate) struct StageLabel(AggregationRound);

impl From<AggregationRound> for StageLabel {
    fn from(round: AggregationRound) -> Self {
        Self(round)
    }
}

impl std::fmt::Display for StageLabel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self.0 {
            AggregationRound::BasicCircuits => "basic_circuits",
            AggregationRound::LeafAggregation => "leaf_aggregation",
            AggregationRound::NodeAggregation => "node_aggregation",
            AggregationRound::Scheduler => "scheduler",
        })
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri_witness_generator")]
pub(crate) struct WitnessGeneratorMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_fetch_time: Family<StageLabel, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub prepare_job_time: Family<StageLabel, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub witness_generation_time: Family<StageLabel, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_save_time: Family<StageLabel, Histogram<Duration>>,
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
    #[metrics(labels = ["stage"])]
    pub latency: LabeledFamily<&'static str, Gauge<Duration>>,
}

#[vise::register]
pub(crate) static SERVER_METRICS: vise::Global<ServerMetrics> = vise::Global::new();
