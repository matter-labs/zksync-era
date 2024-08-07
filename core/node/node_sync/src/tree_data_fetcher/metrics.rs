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
    diamond_proxy_address: Option<String>,
}

impl From<&TreeDataFetcher> for TreeDataFetcherInfo {
    fn from(fetcher: &TreeDataFetcher) -> Self {
        Self {
            poll_interval: fetcher.poll_interval.into(),
            diamond_proxy_address: fetcher
                .diamond_proxy_address
                .map(|addr| format!("{addr:?}")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum ProcessingStage {
    FetchL1CommitEvent,
    FetchBatchDetailsRpc,
    /// Total latency for all clients.
    Fetch,
    Persistence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "kind", rename_all = "snake_case")]
pub(super) enum StepOutcomeLabel {
    UpdatedBatch,
    NoProgress,
    RemoteHashMissing,
    PossibleReorg,
    TransientError,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "source", rename_all = "snake_case")]
pub(super) enum TreeDataProviderSource {
    L1CommitEvent,
    BatchDetailsRpc,
}

const BLOCK_DIFF_BUCKETS: Buckets = Buckets::values(&[
    10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0, 50_000.0,
]);

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
    /// Number of steps during binary search of the L1 commit block number.
    #[metrics(buckets = Buckets::linear(0.0..=32.0, 2.0))]
    pub l1_commit_block_number_binary_search_steps: Histogram<usize>,
    /// Difference between the "from" block specified in the event filter and the L1 block number of the fetched event.
    /// Large values here can signal that fetching data from L1 can break because the filter won't get necessary events.
    #[metrics(buckets = BLOCK_DIFF_BUCKETS)]
    pub l1_commit_block_number_from_diff: Histogram<u64>,
    /// Number of root hashes fetched from a particular source.
    pub root_hash_sources: Family<TreeDataProviderSource, Counter>,
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
            Ok(StepOutcome::PossibleReorg) => StepOutcomeLabel::PossibleReorg,
            Err(err) if err.is_retriable() => StepOutcomeLabel::TransientError,
            Err(_) => return, // fatal error; the node will exit soon anyway
        };
        self.step_outcomes[&label].inc();
    }
}

#[vise::register]
pub(super) static METRICS: vise::Global<TreeDataFetcherMetrics> = vise::Global::new();
