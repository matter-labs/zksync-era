//! Metrics for the synchronization layer of external node.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics};
use zksync_types::aggregated_operations::AggregatedActionType;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum FetchStage {
    GetL1BatchDetails,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, EncodeLabelValue, EncodeLabelSet,
)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum L1BatchStage {
    Open,
    Committed,
    Proven,
    Executed,
}

impl From<AggregatedActionType> for L1BatchStage {
    fn from(ty: AggregatedActionType) -> Self {
        match ty {
            AggregatedActionType::Commit => Self::Committed,
            AggregatedActionType::PublishProofOnchain => Self::Proven,
            AggregatedActionType::Execute => Self::Executed,
            AggregatedActionType::Tee => {
                panic!("AggregatedActionType::Tee is not supported.")
            }
        }
    }
}

/// Metrics for the fetcher.
#[derive(Debug, Metrics)]
#[metrics(prefix = "external_node_fetcher")]
pub(super) struct FetcherMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub requests: Family<FetchStage, Histogram<Duration>>,
    pub l1_batch: Family<L1BatchStage, Gauge<u64>>,
    // uses legacy naming for L2 blocks for compatibility reasons
    pub miniblock: Gauge<u64>,
}

#[vise::register]
pub(super) static FETCHER_METRICS: vise::Global<FetcherMetrics> = vise::Global::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "external_node_action_queue")]
pub(super) struct ActionQueueMetrics {
    pub action_queue_size: Gauge<usize>,
}

#[vise::register]
pub(super) static QUEUE_METRICS: vise::Global<ActionQueueMetrics> = vise::Global::new();
