use std::time::Duration;
use vise::{EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct CircuitLabels {
    pub circuit_type: &'static str,
    pub aggregation_round: AggregationRound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub(crate) enum AggregationRound {
    BasicCircuits,
    LeafAggregation,
    NodeAggregation,
    Scheduler,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri_prover")]
pub(crate) struct ProverFriUtilsMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_fetch_time: Family<CircuitLabels, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static PROVER_FRI_UTILS_METRICS: vise::Global<ProverFriUtilsMetrics> =
    vise::Global::new();
