//! Metrics for the JSON-RPC server.

use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, LabeledFamily,
    LatencyObserver, Metrics,
};

use std::{
    fmt,
    time::{Duration, Instant},
};

use zksync_types::api;

use super::ApiTransport;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "scheme", rename_all = "UPPERCASE")]
pub(super) enum ApiTransportLabel {
    Http,
    Ws,
}

impl From<&ApiTransport> for ApiTransportLabel {
    fn from(transport: &ApiTransport) -> Self {
        match transport {
            ApiTransport::Http(_) => Self::Http,
            ApiTransport::WebSocket(_) => Self::Ws,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
enum BlockIdLabel {
    Hash,
    Committed,
    Finalized,
    Latest,
    Earliest,
    Pending,
    Number,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
enum BlockDiffLabel {
    Exact(u32),
    Lt(u32),
    Geq(u32),
}

impl fmt::Display for BlockDiffLabel {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exact(value) => write!(formatter, "{value}"),
            Self::Lt(value) => write!(formatter, "<{value}"),
            Self::Geq(value) => write!(formatter, ">={value}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct MethodLabels {
    method: &'static str,
    block_id: Option<BlockIdLabel>,
    block_diff: Option<BlockDiffLabel>,
}

impl MethodLabels {
    #[must_use]
    fn with_block_id(mut self, block_id: api::BlockId) -> Self {
        self.block_id = Some(match block_id {
            api::BlockId::Hash(_) => BlockIdLabel::Hash,
            api::BlockId::Number(api::BlockNumber::Number(_)) => BlockIdLabel::Number,
            api::BlockId::Number(api::BlockNumber::Committed) => BlockIdLabel::Committed,
            api::BlockId::Number(api::BlockNumber::Finalized) => BlockIdLabel::Finalized,
            api::BlockId::Number(api::BlockNumber::Latest) => BlockIdLabel::Latest,
            api::BlockId::Number(api::BlockNumber::Earliest) => BlockIdLabel::Earliest,
            api::BlockId::Number(api::BlockNumber::Pending) => BlockIdLabel::Pending,
        });
        self
    }

    #[must_use]
    fn with_block_diff(mut self, block_diff: u32) -> Self {
        self.block_diff = Some(match block_diff {
            0..=2 => BlockDiffLabel::Exact(block_diff),
            3..=9 => BlockDiffLabel::Lt(10),
            10..=99 => BlockDiffLabel::Lt(100),
            100..=999 => BlockDiffLabel::Lt(1_000),
            _ => BlockDiffLabel::Geq(1_000),
        });
        self
    }
}

impl From<&'static str> for MethodLabels {
    fn from(method: &'static str) -> Self {
        Self {
            method,
            block_id: None,
            block_diff: None,
        }
    }
}

#[must_use = "Should be `observe()`d"]
#[derive(Debug)]
pub(super) struct BlockCallObserver<'a> {
    metrics: &'a ApiMetrics,
    start: Instant,
    partial_labels: MethodLabels,
}

impl BlockCallObserver<'_> {
    pub fn observe(self, block_diff: u32) {
        let labels = self.partial_labels.with_block_diff(block_diff);
        let elapsed = self.start.elapsed();
        self.metrics.web3_call[&labels].observe(elapsed);
        self.metrics.web3_call_block_diff[&labels.method].observe(elapsed);
    }

    pub fn observe_without_diff(self) {
        let elapsed = self.start.elapsed();
        self.metrics.web3_call[&self.partial_labels].observe(elapsed);
    }
}

/// General-purpose API server metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "api")]
pub(super) struct ApiMetrics {
    /// Latency of a Web3 call. Calls that take block ID as an input have block ID and block diff
    /// labels (the latter is the difference between the latest sealed miniblock and the resolved miniblock).
    #[metrics(buckets = Buckets::LATENCIES)]
    web3_call: Family<MethodLabels, Histogram<Duration>>,
    /// Difference between the latest sealed miniblock and the resolved miniblock for a web3 call.
    #[metrics(buckets = Buckets::LATENCIES, labels = ["method"])]
    web3_call_block_diff: LabeledFamily<&'static str, Histogram<Duration>>,

    /// Number of internal errors grouped by the Web3 method.
    #[metrics(labels = ["method"])]
    pub web3_internal_errors: LabeledFamily<&'static str, Counter>,
    /// Number of transaction submission errors for a specific submission error reason.
    #[metrics(labels = ["reason"])]
    pub submit_tx_error: LabeledFamily<&'static str, Counter>,
    #[metrics(buckets = Buckets::linear(0.0..=10.0, 1.0))]
    pub web3_in_flight_requests: Family<ApiTransportLabel, Histogram<usize>>,
    /// Number of currently open WebSocket sessions.
    pub ws_open_sessions: Gauge,
}

impl ApiMetrics {
    /// Starts observing a latency of a Web3 call.
    pub fn start_call(&self, method: &'static str) -> LatencyObserver<'_> {
        self.web3_call[&method.into()].start()
    }

    /// Starts observing a latency of a Web3 call that has [`api::BlockId`] as one of its inputs.
    pub fn start_block_call(
        &self,
        method: &'static str,
        block_id: api::BlockId,
    ) -> BlockCallObserver<'_> {
        let partial_labels = MethodLabels::from(method).with_block_id(block_id);
        BlockCallObserver {
            metrics: self,
            start: Instant::now(),
            partial_labels,
        }
    }
}

#[vise::register]
pub(super) static API_METRICS: vise::Global<ApiMetrics> = vise::Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "subscription_type", rename_all = "snake_case")]
pub(super) enum SubscriptionType {
    Blocks,
    Txs,
    Logs,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_web3_pubsub")]
pub(super) struct PubSubMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub db_poll_latency: Family<SubscriptionType, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub notify_subscribers_latency: Family<SubscriptionType, Histogram<Duration>>,
    pub notify: Family<SubscriptionType, Counter>,
    pub active_subscribers: Family<SubscriptionType, Gauge>,
}

#[vise::register]
pub(super) static PUB_SUB_METRICS: vise::Global<PubSubMetrics> = vise::Global::new();
