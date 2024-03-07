use std::thread;
use std::time::Duration;
use vise::{Buckets, Histogram, LabeledFamily, LatencyObserver, Metrics};

/// Request-related DB metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "sql")]
pub(crate) struct RequestMetrics {
    /// Latency of a DB request.
    #[metrics(buckets = Buckets::LATENCIES, labels = ["method"])]
    pub request: LabeledFamily<&'static str, Histogram<Duration>>,
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
