//! Metrics for health checks.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub(crate) enum CheckResult {
    Slow,
    TimedOut,
    Dropped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
struct AbnormalCheckLabels {
    component: &'static str,
    result: CheckResult,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "healthcheck")]
pub(crate) struct HealthMetrics {
    /// Latency for abnormal checks. Includes slow, dropped and timed out checks (distinguished by the "result" label);
    /// skips normal checks.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    abnormal_check_latency: Family<AbnormalCheckLabels, Histogram<Duration>>,
}

impl HealthMetrics {
    pub fn observe_abnormal_check(
        &self,
        component: &'static str,
        result: CheckResult,
        duration: Duration,
    ) {
        let labels = AbnormalCheckLabels { component, result };
        self.abnormal_check_latency[&labels].observe(duration);
    }
}

#[vise::register]
pub(crate) static METRICS: vise::Global<HealthMetrics> = vise::Global::new();
