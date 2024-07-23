use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};
use zksync_types::basic_fri_types::AggregationRound;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
pub struct CircuitLabels {
    pub circuit_type: u8,
    pub aggregation_round: StageLabel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", format = "wit_gen_{}")]
pub struct StageLabel(AggregationRound);

impl From<AggregationRound> for StageLabel {
    fn from(round: AggregationRound) -> Self {
        Self(round)
    }
}

impl std::fmt::Display for StageLabel {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
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
