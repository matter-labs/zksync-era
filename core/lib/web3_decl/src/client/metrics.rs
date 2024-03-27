//! L2 client metrics.

use std::time::Duration;

use vise::{Buckets, Counter, EncodeLabelSet, Family, Histogram, Metrics, Unit};

use super::{AcquireStats, RateLimitOrigin};

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct RequestLabels {
    pub component: &'static str,
    pub method: String,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "l2_client")]
pub(super) struct L2ClientMetrics {
    /// Number of requests timed out in the rate-limiting logic.
    rate_limit_timeout: Family<RequestLabels, Counter>,
    /// Latency of rate-limiting logic for rate-limited requests.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    rate_limit_latency: Family<RequestLabels, Histogram<Duration>>,
}

impl L2ClientMetrics {
    pub fn observe_rate_limit_latency(
        &self,
        component: &'static str,
        origin: RateLimitOrigin,
        stats: &AcquireStats,
    ) {
        for method in origin.distinct_method_names() {
            let request_labels = RequestLabels {
                component,
                method: method.to_owned(),
            };
            METRICS.rate_limit_latency[&request_labels].observe(stats.total_sleep_time);
        }
    }

    pub fn observe_rate_limit_timeout(&self, component: &'static str, origin: RateLimitOrigin) {
        for method in origin.distinct_method_names() {
            let request_labels = RequestLabels {
                component,
                method: method.to_owned(),
            };
            METRICS.rate_limit_timeout[&request_labels].inc();
        }
    }
}

#[vise::register]
pub(super) static METRICS: vise::Global<L2ClientMetrics> = vise::Global::new();
