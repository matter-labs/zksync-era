//! Metrics for Tree data fetcher.

use std::time::Duration;

use vise::{
    Buckets, Counter, DurationAsSecs, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram,
    Info, Metrics, Unit,
};

use super::{StepOutcome, TreeDataFetcher, TreeDataFetcherError};

#[derive(Debug, EncodeLabelSet)]
struct TreeDataFetcherInfo {
    #[metrics(unit = Unit::Seconds)]
    poll_interval: DurationAsSecs,
}

impl From<&TreeDataFetcher> for TreeDataFetcherInfo {
    fn from(fetcher: &TreeDataFetcher) -> Self {
        Self {
            poll_interval: fetcher.poll_interval.into(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum ProcessingStage {
    Fetch,
    Persistence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "kind", rename_all = "snake_case")]
pub(super) enum StepOutcomeLabel {
    UpdatedBatch,
    NoProgress,
    RemoteHashMissing,
    TransientError,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "external_node_tree_data_fetcher")]
pub(super) struct TreeDataFetcherMetrics {
    /// Immutable information about the fetcher.
    info: Info<TreeDataFetcherInfo>,
    /// Number of times a fetcher step resulted in a certain outcome.
    pub step_outcomes: Family<StepOutcomeLabel, Counter>,
    /// Last L1 batch with tree data updated by the fetcher.
    pub last_updated_batch_number: Gauge<u64>,
    /// Latency of a particular stage of processing a single L1 batch.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub stage_latency: Family<ProcessingStage, Histogram<Duration>>,
}

impl TreeDataFetcherMetrics {
    pub fn observe_info(&self, fetcher: &TreeDataFetcher) {
        let info = TreeDataFetcherInfo::from(fetcher);
        tracing::info!("Starting tree data fetcher with {info:?}");
        if let Err(err) = self.info.set(info) {
            tracing::warn!(
                "Error setting configuration info {:?} for tree data fetcher; already set to {:?}",
                err.into_inner(),
                self.info.get()
            );
        }
    }

    pub fn observe_step_outcome(&self, outcome: Result<&StepOutcome, &TreeDataFetcherError>) {
        let label = match outcome {
            Ok(StepOutcome::UpdatedBatch(number)) => {
                self.last_updated_batch_number.set(number.0.into());
                StepOutcomeLabel::UpdatedBatch
            }
            Ok(StepOutcome::NoProgress) => StepOutcomeLabel::NoProgress,
            Ok(StepOutcome::RemoteHashMissing) => StepOutcomeLabel::RemoteHashMissing,
            Err(err) if err.is_transient() => StepOutcomeLabel::TransientError,
            Err(_) => return, // fatal error; the node will exit soon anyway
        };
        self.step_outcomes[&label].inc();
    }
}

#[vise::register]
pub(super) static METRICS: vise::Global<TreeDataFetcherMetrics> = vise::Global::new();
