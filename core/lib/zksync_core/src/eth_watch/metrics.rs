//! Metrics for Ethereum watcher.

use vise::{Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum PollStage {
    Request,
    PersistL1Txs,
    PersistUpgrades,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_eth_watch")]
pub(super) struct EthWatcherMetrics {
    pub eth_poll: Counter,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub poll_eth_node: Family<PollStage, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub get_priority_op_events: Histogram<Duration>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<EthWatcherMetrics> = vise::Global::new();
