//! Metrics for the data access layer.

use std::{thread, time::Duration};

use anyhow::Context as _;
use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, LabeledFamily,
    LatencyObserver, Metrics, Unit,
};

use crate::ConnectionPool;

/// Request-related DB metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "sql")]
pub(crate) struct RequestMetrics {
    /// Latency of a DB request.
    #[metrics(buckets = Buckets::LATENCIES, labels = ["method"])]
    pub request: LabeledFamily<&'static str, Histogram<Duration>>,
    /// Counter of slow DB requests.
    #[metrics(labels = ["method"])]
    pub request_slow: LabeledFamily<&'static str, Counter>,
    /// Counter of errored DB requests.
    #[metrics(labels = ["method"])]
    pub request_error: LabeledFamily<&'static str, Counter>,
}

#[vise::register]
pub(crate) static REQUEST_METRICS: vise::Global<RequestMetrics> = vise::Global::new();

/// Reporter of latency for DAL methods consisting of multiple DB queries. If there's a single query,
/// use `.instrument().report_latency()` on it instead.
///
/// Should be created at the start of the relevant method and dropped when the latency needs to be reported.
#[derive(Debug)]
pub(crate) struct MethodLatency(Option<LatencyObserver<'static>>);

impl MethodLatency {
    pub fn new(name: &'static str) -> Self {
        Self(Some(REQUEST_METRICS.request[&name].start()))
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
