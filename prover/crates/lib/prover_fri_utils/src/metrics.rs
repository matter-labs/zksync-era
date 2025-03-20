use vise::{EncodeLabelSet, EncodeLabelValue};
use zksync_types::basic_fri_types::AggregationRound;

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
