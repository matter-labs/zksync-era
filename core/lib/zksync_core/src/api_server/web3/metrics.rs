//! Metrics for the JSON-RPC server.

use std::{fmt, time::Duration};

use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, LabeledFamily,
    Metrics, Unit,
};
use zksync_types::api;
use zksync_web3_decl::error::Web3Error;

use super::{ApiTransport, TypedFilter};

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
    pub fn new(method: &'static str) -> Self {
        Self {
            method,
            block_id: None,
            block_diff: None,
        }
    }

    #[must_use]
    pub fn with_block_id(mut self, block_id: api::BlockId) -> Self {
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
    pub fn with_block_diff(mut self, block_diff: u32) -> Self {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
enum Web3ErrorKind {
    NoBlock,
    Pruned,
    SubmitTransaction,
    TransactionSerialization,
    Proxy,
    TooManyTopics,
    FilterNotFound,
    LogsLimitExceeded,
    InvalidFilterBlockHash,
    TreeApiUnavailable,
    Internal,
}

impl Web3ErrorKind {
    fn new(err: &Web3Error) -> Self {
        match err {
            Web3Error::NoBlock => Self::NoBlock,
            Web3Error::PrunedBlock(_) | Web3Error::PrunedL1Batch(_) => Self::Pruned,
            Web3Error::SubmitTransactionError(..) => Self::SubmitTransaction,
            Web3Error::ProxyError(_) => Self::Proxy,
            Web3Error::SerializationError(_) => Self::TransactionSerialization,
            Web3Error::TooManyTopics => Self::TooManyTopics,
            Web3Error::FilterNotFound => Self::FilterNotFound,
            Web3Error::LogsLimitExceeded(..) => Self::LogsLimitExceeded,
            Web3Error::InvalidFilterBlockHash => Self::InvalidFilterBlockHash,
            Web3Error::TreeApiUnavailable => Self::TreeApiUnavailable,
            Web3Error::InternalError(_) => Self::Internal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
enum ProtocolErrorOrigin {
    App,
    Framework,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
struct ProtocolErrorLabels {
    method: &'static str,
    error_code: i32,
    origin: ProtocolErrorOrigin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
struct Web3ErrorLabels {
    method: &'static str,
    kind: Web3ErrorKind,
}

/// General-purpose API server metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "api")]
pub(super) struct ApiMetrics {
    /// Latency of a Web3 call. Calls that take block ID as an input have block ID and block diff
    /// labels (the latter is the difference between the latest sealed miniblock and the resolved miniblock).
    #[metrics(buckets = Buckets::LATENCIES)]
    web3_call: Family<MethodLabels, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    web3_dropped_call_latency: Family<MethodLabels, Histogram<Duration>>,
    /// Difference between the latest sealed miniblock and the resolved miniblock for a web3 call.
    #[metrics(buckets = Buckets::LATENCIES, labels = ["method"])]
    web3_call_block_diff: LabeledFamily<&'static str, Histogram<Duration>>,
    /// Number of application errors grouped by error kind and method name. Only collected for errors that were successfully routed
    /// to a method (i.e., this method is defined).
    web3_errors: Family<Web3ErrorLabels, Counter>,
    /// Number of protocol errors grouped by error code and method name. Method name is not set for "method not found" errors.
    web3_rpc_errors: Family<ProtocolErrorLabels, Counter>,
    /// Number of transaction submission errors for a specific submission error reason.
    #[metrics(labels = ["reason"])]
    pub submit_tx_error: LabeledFamily<&'static str, Counter>,
    #[metrics(buckets = Buckets::exponential(1.0..=128.0, 2.0))]
    pub web3_in_flight_requests: Family<ApiTransportLabel, Histogram<usize>>,
    /// Number of currently open WebSocket sessions.
    pub ws_open_sessions: Gauge,
}

impl ApiMetrics {
    pub fn observe_latency(&self, labels: &MethodLabels, latency: Duration) {
        self.web3_call[labels].observe(latency);
    }

    pub fn observe_dropped_call(&self, labels: &MethodLabels, latency: Duration) {
        self.web3_dropped_call_latency[labels].observe(latency);
    }

    pub fn observe_protocol_error(&self, method: &'static str, error_code: i32, app_error: bool) {
        let labels = ProtocolErrorLabels {
            method,
            error_code,
            origin: if app_error {
                ProtocolErrorOrigin::App
            } else {
                ProtocolErrorOrigin::Framework
            },
        };
        if self.web3_rpc_errors[&labels].inc() == 0 {
            let ProtocolErrorLabels {
                method,
                error_code,
                origin,
            } = &labels;
            tracing::info!(
                "Observed new error code for method `{method}`: {error_code}, origin={origin:?}"
            );
        }
    }

    pub fn observe_web3_error(&self, method: &'static str, err: &Web3Error) {
        // Log internal error details.
        match err {
            Web3Error::InternalError(err) => {
                tracing::error!("Internal error in method `{method}`: {err}");
            }
            Web3Error::ProxyError(err) => {
                tracing::warn!("Error proxying call to main node in method `{method}`: {err}");
            }
            _ => { /* do nothing */ }
        }

        let labels = Web3ErrorLabels {
            method,
            kind: Web3ErrorKind::new(err),
        };
        if self.web3_errors[&labels].inc() == 0 {
            // Only log the first error with the label to not spam logs.
            tracing::info!(
                "Observed new error type for method `{}`: {:?}",
                labels.method,
                labels.kind
            );
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
    /// Latency to load new events from Postgres before broadcasting them to subscribers.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub db_poll_latency: Family<SubscriptionType, Histogram<Duration>>,
    /// Latency to send an atomic batch of events to a single subscriber.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub notify_subscribers_latency: Family<SubscriptionType, Histogram<Duration>>,
    /// Total number of events sent to all subscribers of a certain type.
    pub notify: Family<SubscriptionType, Counter>,
    /// Number of currently active subscribers split by the subscription type.
    pub active_subscribers: Family<SubscriptionType, Gauge>,
    /// Lifetime of a subscriber of a certain type.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub subscriber_lifetime: Family<SubscriptionType, Histogram<Duration>>,
    /// Current length of the broadcast channel of a certain type. With healthy subscribers, this value
    /// should be reasonably low.
    pub broadcast_channel_len: Family<SubscriptionType, Gauge<usize>>,
    /// Number of skipped broadcast messages.
    #[metrics(buckets = Buckets::exponential(1.0..=128.0, 2.0))]
    pub skipped_broadcast_messages: Family<SubscriptionType, Histogram<u64>>,
    /// Number of subscribers dropped because of a send timeout.
    pub subscriber_send_timeouts: Family<SubscriptionType, Counter>,
}

#[vise::register]
pub(super) static PUB_SUB_METRICS: vise::Global<PubSubMetrics> = vise::Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "type", rename_all = "snake_case")]
pub(super) enum FilterType {
    Events,
    Blocks,
    PendingTransactions,
}

impl From<&TypedFilter> for FilterType {
    fn from(value: &TypedFilter) -> Self {
        match value {
            TypedFilter::Events(_, _) => FilterType::Events,
            TypedFilter::Blocks(_) => FilterType::Blocks,
            TypedFilter::PendingTransactions(_) => FilterType::PendingTransactions,
        }
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_web3_filter")]
pub(super) struct FilterMetrics {
    /// Number of currently active filters grouped by the filter type
    pub filter_count: Family<FilterType, Gauge>,
    /// Time in seconds between consecutive requests to the filter grouped by the filter type
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub request_frequency: Family<FilterType, Histogram<Duration>>,
    /// Lifetime of a filter in seconds grouped by the filter type
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub filter_lifetime: Family<FilterType, Histogram<Duration>>,
    /// Number of requests to the filter grouped by the filter type
    #[metrics(buckets = Buckets::exponential(1.0..=1048576.0, 2.0))]
    pub request_count: Family<FilterType, Histogram<usize>>,
}

#[vise::register]
pub(super) static FILTER_METRICS: vise::Global<FilterMetrics> = vise::Global::new();
