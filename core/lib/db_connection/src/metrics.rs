use std::{thread, time::Duration};

use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Histogram, LabeledFamily,
    LatencyObserver, Metrics, MetricsFamily, Unit,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct RequestLabels {
    method: &'static str,
}

impl From<&'static str> for RequestLabels {
    fn from(method: &'static str) -> Self {
        Self { method }
    }
}

/// Request-related DB metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "sql")]
pub(crate) struct RequestMetrics {
    /// Latency of a DB request.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub request: Histogram<Duration>,
    /// Counter of slow DB requests.
    pub request_slow: Counter,
    /// Counter of errored DB requests.
    pub request_error: Counter,
}

#[vise::register]
pub(crate) static REQUEST_METRICS: MetricsFamily<RequestLabels, RequestMetrics> =
    MetricsFamily::new();

/// Reporter of latency for DAL methods consisting of multiple DB queries. If there's a single query,
/// use `.instrument().report_latency()` on it instead.
///
/// Should be created at the start of the relevant method and dropped when the latency needs to be reported.
#[derive(Debug)]
pub struct MethodLatency(Option<LatencyObserver<'static>>);

impl MethodLatency {
    pub fn new(name: &'static str) -> Self {
        Self(Some(REQUEST_METRICS[&name.into()].request.start()))
    }
}

impl Drop for MethodLatency {
    fn drop(&mut self) {
        if !thread::panicking() {
            let observer = self.0.take().unwrap();
            // `unwrap()` is safe; the observer is only taken out on drop
            observer.observe();
        }
    }
}

/// Kind of a connection error.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "kind", rename_all = "snake_case")]
pub(crate) enum ConnectionErrorKind {
    Timeout,
    Database,
    Io,
    Other,
}

impl From<&sqlx::Error> for ConnectionErrorKind {
    fn from(err: &sqlx::Error) -> Self {
        match err {
            sqlx::Error::PoolTimedOut => Self::Timeout,
            sqlx::Error::Database(_) => Self::Database,
            sqlx::Error::Io(_) => Self::Io,
            _ => Self::Other,
        }
    }
}

const POOL_SIZE_BUCKETS: Buckets = Buckets::linear(0.0..=100.0, 10.0);

/// Connection-related metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "sql_connection")]
pub(crate) struct ConnectionMetrics {
    /// Latency of acquiring a DB connection.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub acquire: Histogram<Duration>,
    /// Latency of acquiring a DB connection, tagged with the requester label.
    #[metrics(buckets = Buckets::LATENCIES, labels = ["requester"])]
    pub acquire_tagged: LabeledFamily<&'static str, Histogram<Duration>>,
    /// Current DB pool size.
    #[metrics(buckets = POOL_SIZE_BUCKETS)]
    pub pool_size: Histogram<usize>,
    /// Current number of idle connections in the DB pool.
    #[metrics(buckets = POOL_SIZE_BUCKETS)]
    pub pool_idle: Histogram<usize>,
    /// Number of errors occurred when acquiring a DB connection.
    pub pool_acquire_error: Family<ConnectionErrorKind, Counter>,
    /// Lifetime of a DB connection, tagged with the requester label.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds, labels = ["requester"])]
    pub lifetime: LabeledFamily<&'static str, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static CONNECTION_METRICS: vise::Global<ConnectionMetrics> = vise::Global::new();
