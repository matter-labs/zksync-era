//! Metrics for the JSON-RPC server.

use std::{borrow::Cow, fmt, time::Duration};

use vise::{
    Buckets, Counter, DurationAsSecs, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram,
    Info, LabeledFamily, Metrics, MetricsFamily, Unit,
};
use zksync_types::api;
use zksync_web3_decl::error::Web3Error;

use super::{
    backend_jsonrpsee::MethodMetadata, ApiTransport, InternalApiConfig, OptionalApiParams,
    TypedFilter,
};
use crate::{tx_sender::SubmitTxError, utils::ReportFilter};

/// Observed version of RPC parameters. Have a bounded upper-limit size (256 bytes), so that we don't over-allocate.
#[derive(Debug)]
pub(super) enum ObservedRpcParams<'a> {
    None,
    Unknown,
    Borrowed(&'a serde_json::value::RawValue),
    Owned { start: Box<str>, total_len: usize },
}

impl fmt::Display for ObservedRpcParams<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if matches!(self, Self::Unknown) {
            return Ok(());
        }
        formatter.write_str(" with params ")?;

        let (start, total_len) = match self {
            Self::None => return formatter.write_str("[]"),
            Self::Unknown => unreachable!(),
            Self::Borrowed(params) => (Self::maybe_shorten(params), params.get().len()),
            Self::Owned { start, total_len } => (start.as_ref(), *total_len),
        };

        if total_len == start.len() {
            formatter.write_str(start)
        } else {
            // Since params is a JSON array, we add a closing ']' at the end
            write!(formatter, "{start} ...({total_len} bytes)]")
        }
    }
}

impl<'a> ObservedRpcParams<'a> {
    const MAX_LEN: usize = 256;

    fn maybe_shorten(raw_value: &serde_json::value::RawValue) -> &str {
        let raw_str = raw_value.get();
        if raw_str.len() <= Self::MAX_LEN {
            raw_str
        } else {
            // Truncate `params_str` to be no longer than `MAX_LEN`.
            let mut pos = Self::MAX_LEN;
            while !raw_str.is_char_boundary(pos) {
                pos -= 1; // Shouldn't underflow; the char boundary is at most 3 bytes away
            }
            &raw_str[..pos]
        }
    }

    pub fn new(raw: Option<&Cow<'a, serde_json::value::RawValue>>) -> Self {
        match raw {
            None => Self::None,
            // In practice, `jsonrpsee` never returns `Some(Cow::Borrowed(_))` because of a `serde` / `serde_json` flaw (?)
            // when deserializing `Cow<'_, RawValue`: https://github.com/serde-rs/json/issues/1076. Thus, each `new()` call
            // in which params are actually specified will allocate, but this allocation is quite small.
            Some(Cow::Borrowed(params)) => Self::Borrowed(params),
            Some(Cow::Owned(params)) => Self::Owned {
                start: Self::maybe_shorten(params).into(),
                total_len: params.get().len(),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "scheme", rename_all = "UPPERCASE")]
pub(crate) enum ApiTransportLabel {
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
    FastFinalized,
    Latest,
    L1Committed,
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
struct MethodLabels {
    method: &'static str,
    block_id: Option<BlockIdLabel>,
    block_diff: Option<BlockDiffLabel>,
}

impl From<&MethodMetadata> for MethodLabels {
    fn from(meta: &MethodMetadata) -> Self {
        let block_id = meta.block_id.map(|block_id| match block_id {
            api::BlockId::Hash(_) => BlockIdLabel::Hash,
            api::BlockId::Number(api::BlockNumber::Number(_)) => BlockIdLabel::Number,
            api::BlockId::Number(api::BlockNumber::Committed) => BlockIdLabel::Committed,
            api::BlockId::Number(api::BlockNumber::Finalized) => BlockIdLabel::Finalized,
            api::BlockId::Number(api::BlockNumber::FastFinalized) => BlockIdLabel::FastFinalized,
            api::BlockId::Number(api::BlockNumber::Latest) => BlockIdLabel::Latest,
            api::BlockId::Number(api::BlockNumber::L1Committed) => BlockIdLabel::L1Committed,
            api::BlockId::Number(api::BlockNumber::Earliest) => BlockIdLabel::Earliest,
            api::BlockId::Number(api::BlockNumber::Pending) => BlockIdLabel::Pending,
        });
        let block_diff = meta.block_diff.map(|block_diff| match block_diff {
            0..=2 => BlockDiffLabel::Exact(block_diff),
            3..=9 => BlockDiffLabel::Lt(10),
            10..=99 => BlockDiffLabel::Lt(100),
            100..=999 => BlockDiffLabel::Lt(1_000),
            _ => BlockDiffLabel::Geq(1_000),
        });
        Self {
            method: meta.name,
            block_id,
            block_diff,
        }
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
            Web3Error::InternalError(_)
            | Web3Error::MethodNotImplemented
            | Web3Error::ServerShuttingDown => Self::Internal,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
struct SubmitErrorLabels {
    method: &'static str,
    reason: &'static str,
}

#[derive(Debug, EncodeLabelSet)]
struct Web3ConfigLabels {
    #[metrics(unit = Unit::Seconds)]
    polling_interval: DurationAsSecs,
    req_entities_limit: usize,
    fee_history_limit: u64,
    filters_limit: Option<usize>,
    subscriptions_limit: Option<usize>,
    #[metrics(unit = Unit::Bytes)]
    batch_request_size_limit: Option<usize>,
    #[metrics(unit = Unit::Bytes)]
    response_body_size_limit: Option<usize>,
    websocket_requests_per_minute_limit: Option<u32>,
}

/// Roughly exponential buckets for the `web3_call_block_diff` metric. The distribution should be skewed towards lower values.
const SMALL_COUNT_BUCKETS: Buckets = Buckets::values(&[
    0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0,
]);

const RESPONSE_SIZE_BUCKETS: Buckets = Buckets::exponential(1.0..=1_048_576.0, 4.0);

/// General-purpose API server metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "api")]
pub(crate) struct ApiMetrics {
    /// Web3 server configuration.
    web3_info: Family<ApiTransportLabel, Info<Web3ConfigLabels>>,

    /// Latency of a Web3 call. Calls that take block ID as an input have block ID and block diff
    /// labels (the latter is the difference between the latest sealed L2 block and the resolved L2 block).
    #[metrics(buckets = Buckets::LATENCIES)]
    web3_call: Family<MethodLabels, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    web3_dropped_call_latency: Family<MethodLabels, Histogram<Duration>>,
    /// Difference between the latest sealed L2 block and the resolved L2 block for a web3 call.
    #[metrics(buckets = SMALL_COUNT_BUCKETS, labels = ["method"])]
    web3_call_block_diff: LabeledFamily<&'static str, Histogram<u64>>,
    /// Serialized response size in bytes. Only recorded for successful responses.
    #[metrics(buckets = RESPONSE_SIZE_BUCKETS, labels = ["method"], unit = Unit::Bytes)]
    web3_call_response_size: LabeledFamily<&'static str, Histogram<usize>>,

    /// Number of application errors grouped by error kind and method name. Only collected for errors that were successfully routed
    /// to a method (i.e., this method is defined).
    web3_errors: Family<Web3ErrorLabels, Counter>,
    /// Number of protocol errors grouped by error code and method name. Method name is not set for "method not found" errors.
    web3_rpc_errors: Family<ProtocolErrorLabels, Counter>,
    /// Number of transaction submission errors for a specific submission error reason.
    submit_tx_error: Family<SubmitErrorLabels, Counter>,

    #[metrics(buckets = Buckets::exponential(1.0..=128.0, 2.0))]
    pub web3_in_flight_requests: Family<ApiTransportLabel, Histogram<usize>>,
    /// Number of currently open WebSocket sessions.
    pub ws_open_sessions: Gauge,
    /// Number of currently inserted into DB transactions.
    pub inflight_tx_submissions: Gauge,
}

impl ApiMetrics {
    pub(super) fn observe_config(
        &self,
        transport: ApiTransportLabel,
        polling_interval: Duration,
        config: &InternalApiConfig,
        optional: &OptionalApiParams,
    ) {
        let config_labels = Web3ConfigLabels {
            polling_interval: polling_interval.into(),
            req_entities_limit: config.req_entities_limit,
            fee_history_limit: config.fee_history_limit,
            filters_limit: optional.filters_limit,
            subscriptions_limit: optional.subscriptions_limit,
            batch_request_size_limit: optional.batch_request_size_limit,
            response_body_size_limit: optional
                .response_body_size_limit
                .as_ref()
                .map(|limit| limit.global),
            websocket_requests_per_minute_limit: optional
                .websocket_requests_per_minute_limit
                .map(Into::into),
        };
        tracing::info!("{transport:?} Web3 server is configured with options: {config_labels:?}");
        if self.web3_info[&transport].set(config_labels).is_err() {
            tracing::warn!("Cannot set config labels for {transport:?} Web3 server");
        }
    }

    /// Observes latency of a finished RPC call.
    pub(super) fn observe_latency(
        &self,
        meta: &MethodMetadata,
        raw_params: &ObservedRpcParams<'_>,
    ) {
        static FILTER: ReportFilter = report_filter!(Duration::from_secs(1));
        const MIN_REPORTED_LATENCY: Duration = Duration::from_secs(5);

        let latency = meta.started_at.elapsed();
        self.web3_call[&MethodLabels::from(meta)].observe(latency);
        if let Some(block_diff) = meta.block_diff {
            self.web3_call_block_diff[&meta.name].observe(block_diff.into());
        }
        if latency >= MIN_REPORTED_LATENCY && FILTER.should_report() {
            tracing::info!("Long call to `{}`{raw_params}: {latency:?}", meta.name);
        }
    }

    /// Observes latency of a dropped RPC call.
    pub(super) fn observe_dropped_call(
        &self,
        meta: &MethodMetadata,
        raw_params: &ObservedRpcParams<'_>,
    ) {
        static FILTER: ReportFilter = report_filter!(Duration::from_secs(1));

        let latency = meta.started_at.elapsed();
        self.web3_dropped_call_latency[&MethodLabels::from(meta)].observe(latency);
        if FILTER.should_report() {
            tracing::info!(
                "Call to `{}`{raw_params} was dropped by client after {latency:?}",
                meta.name
            );
        }
    }

    /// Observes serialized size of a response.
    pub(super) fn observe_response_size(
        &self,
        method: &'static str,
        raw_params: &ObservedRpcParams<'_>,
        size: usize,
    ) {
        static FILTER: ReportFilter = report_filter!(Duration::from_secs(1));
        const MIN_REPORTED_SIZE: usize = 4 * 1_024 * 1_024; // 4 MiB

        self.web3_call_response_size[&method].observe(size);
        if size >= MIN_REPORTED_SIZE && FILTER.should_report() {
            tracing::info!(
                "Call to `{method}`{raw_params} has resulted in large response: {size}B"
            );
        }
    }

    pub(super) fn observe_protocol_error(
        &self,
        method: &'static str,
        raw_params: &ObservedRpcParams<'_>,
        error_code: i32,
        has_app_error: bool,
    ) {
        static FILTER: ReportFilter = report_filter!(Duration::from_millis(100));

        let labels = ProtocolErrorLabels {
            method,
            error_code,
            origin: if has_app_error {
                ProtocolErrorOrigin::App
            } else {
                ProtocolErrorOrigin::Framework
            },
        };
        if self.web3_rpc_errors[&labels].inc() == 0 || FILTER.should_report() {
            let ProtocolErrorLabels {
                method,
                error_code,
                origin,
            } = &labels;
            tracing::info!(
                "Observed error code {error_code} (origin: {origin:?}) for method `{method}`{raw_params}"
            );
        }
    }

    pub(super) fn observe_web3_error(&self, method: &'static str, err: &Web3Error) {
        // Log internal error details.
        match err {
            Web3Error::InternalError(err) => {
                tracing::error!("Internal error in method `{method}`: {err:#}");
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

    pub(super) fn observe_submit_error(&self, method: &'static str, err: &SubmitTxError) {
        static FILTER: ReportFilter = report_filter!(Duration::from_secs(5));

        // All internal errors are reported anyway, so no need to log them here.
        if !matches!(err, SubmitTxError::Internal(_)) && FILTER.should_report() {
            tracing::info!("Observed submission error for method `{method}`: {err}");
        }

        let labels = SubmitErrorLabels {
            method,
            reason: err.prom_error_code(),
        };
        self.submit_tx_error[&labels].inc();
    }
}

#[vise::register]
pub(crate) static API_METRICS: vise::Global<ApiMetrics> = vise::Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "subscription_type", rename_all = "snake_case")]
pub enum SubscriptionType {
    Blocks,
    Txs,
    Logs,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_web3_pubsub")]
pub(super) struct PubSubMetrics {
    /// Latency to load new events from Postgres before broadcasting them to subscribers.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub db_poll_latency: Histogram<Duration>,
    /// Latency to send an atomic batch of events to a single subscriber.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub notify_subscribers_latency: Histogram<Duration>,
    /// Total number of events sent to all subscribers of a certain type.
    pub notify: Counter,
    /// Number of currently active subscribers split by the subscription type.
    pub active_subscribers: Gauge,
    /// Lifetime of a subscriber of a certain type.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub subscriber_lifetime: Histogram<Duration>,
    /// Current length of the broadcast channel of a certain type. With healthy subscribers, this value
    /// should be reasonably low.
    pub broadcast_channel_len: Gauge<usize>,
    /// Number of skipped broadcast messages.
    #[metrics(buckets = Buckets::exponential(1.0..=128.0, 2.0))]
    pub skipped_broadcast_messages: Histogram<u64>,
    /// Number of subscribers dropped because of a send timeout.
    pub subscriber_send_timeouts: Counter,
}

#[vise::register]
pub(super) static PUB_SUB_METRICS: MetricsFamily<SubscriptionType, PubSubMetrics> =
    MetricsFamily::new();

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
    pub filter_count: Gauge,
    /// Time in seconds between consecutive requests to the filter grouped by the filter type
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub request_frequency: Histogram<Duration>,
    /// Lifetime of a filter in seconds grouped by the filter type
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub filter_lifetime: Histogram<Duration>,
    /// Number of requests to the filter grouped by the filter type
    #[metrics(buckets = Buckets::exponential(1.0..=1048576.0, 2.0))]
    pub request_count: Histogram<usize>,
}

#[vise::register]
pub(super) static FILTER_METRICS: MetricsFamily<FilterType, FilterMetrics> = MetricsFamily::new();

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_mempool_cache")]
pub(super) struct MempoolCacheMetrics {
    /// Latency of mempool cache updates - the time it takes to load all the new transactions from the DB.
    /// Does not include cache update time
    #[metrics(buckets = Buckets::LATENCIES)]
    pub db_poll_latency: Histogram<Duration>,
    /// Number of transactions loaded from the DB during the last cache update
    #[metrics(buckets = Buckets::exponential(1.0..=2048.0, 2.0))]
    pub tx_batch_size: Histogram<usize>,
}

#[vise::register]
pub(super) static MEMPOOL_CACHE_METRICS: vise::Global<MempoolCacheMetrics> = vise::Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum TxReceiptStage {
    AccountTypes,
    StoredNonces,
    DeploymentEvents,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_web3_tx_receipts")]
pub(super) struct TxReceiptMetrics {
    /// Total number of processed receipts per query.
    #[metrics(buckets = SMALL_COUNT_BUCKETS)]
    pub total_count: Histogram<usize>,
    /// Number of distinct addresses queried.
    #[metrics(buckets = SMALL_COUNT_BUCKETS)]
    pub initiator_address_count: Histogram<usize>,
    /// Number of receipts for which nonces should be computed, per query.
    #[metrics(buckets = SMALL_COUNT_BUCKETS)]
    pub unknown_nonces_count: Histogram<usize>,
    /// Latency of a particular receipt processing stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub stage_latency: Family<TxReceiptStage, Histogram<Duration>>,
}

#[vise::register]
pub(super) static TX_RECEIPT_METRICS: vise::Global<TxReceiptMetrics> = vise::Global::new();

#[cfg(test)]
mod tests {
    use assert_matches::assert_matches;
    use serde_json::value::RawValue;

    use super::*;

    #[test]
    fn observing_rpc_params() {
        let rpc_params = ObservedRpcParams::new(None);
        assert_matches!(rpc_params, ObservedRpcParams::None);
        assert_eq!(rpc_params.to_string(), " with params []");

        let raw_params = RawValue::from_string(r#"["0x1"]"#.into()).unwrap();
        let rpc_params = ObservedRpcParams::new(Some(&Cow::Borrowed(&raw_params)));
        assert_matches!(rpc_params, ObservedRpcParams::Borrowed(_));
        assert_eq!(rpc_params.to_string(), r#" with params ["0x1"]"#);

        let rpc_params = ObservedRpcParams::new(Some(&Cow::Owned(raw_params)));
        assert_matches!(rpc_params, ObservedRpcParams::Owned { .. });
        assert_eq!(rpc_params.to_string(), r#" with params ["0x1"]"#);

        let raw_params = [zksync_types::web3::Bytes(vec![0xff; 512])];
        let raw_params = serde_json::value::to_raw_value(&raw_params).unwrap();
        assert_eq!(raw_params.get().len(), 1_030); // 1024 'f' chars + '0x' + '[]' + '""'
        let rpc_params = ObservedRpcParams::new(Some(&Cow::Borrowed(&raw_params)));
        assert_matches!(rpc_params, ObservedRpcParams::Borrowed(_));
        let rpc_params_str = rpc_params.to_string();
        assert!(
            rpc_params_str.starts_with(r#" with params ["0xffff"#),
            "{rpc_params_str}"
        );
        assert!(
            rpc_params_str.ends_with("ff ...(1030 bytes)]"),
            "{rpc_params_str}"
        );

        let rpc_params = ObservedRpcParams::new(Some(&Cow::Owned(raw_params)));
        assert_matches!(rpc_params, ObservedRpcParams::Owned { .. });
        let rpc_params_str = rpc_params.to_string();
        assert!(
            rpc_params_str.starts_with(r#" with params ["0xffff"#),
            "{rpc_params_str}"
        );
        assert!(
            rpc_params_str.ends_with("ff ...(1030 bytes)]"),
            "{rpc_params_str}"
        );
    }
}
