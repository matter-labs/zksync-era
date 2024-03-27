//! L2 client metrics.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, Family, Histogram, Metrics, Unit};

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct RequestLabels {
    pub component: &'static str,
    pub method: String,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "l2_client")]
pub(super) struct L2ClientMetrics {
    /// Latency of rate-limiting logic for rate-limited requests.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub rate_limit_latency: Family<RequestLabels, Histogram<Duration>>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<L2ClientMetrics> = vise::Global::new();
