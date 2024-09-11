//! Metrics for Ethereum watcher.

use std::time::Duration;

use vise::{Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum PollStage {
    PersistL1Txs,
    PersistUpgrades,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_eth_watch")]
pub(super) struct EthWatcherMetrics {
    /// Number of times Ethereum was polled.
    pub eth_poll: Counter,
    /// Latency of polling and processing events split by stage.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub poll_eth_node: Family<PollStage, Histogram<Duration>>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<EthWatcherMetrics> = vise::Global::new();
