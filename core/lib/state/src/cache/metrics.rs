//! General-purpose cache metrics.

use std::time::Duration;

use vise::{Buckets, Counter, EncodeLabelValue, Gauge, Histogram, LabeledFamily, Metrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub(super) enum Method {
    Get,
    Insert,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue)]
#[metrics(rename_all = "snake_case")]
pub(super) enum RequestOutcome {
    Hit,
    Miss,
}

impl RequestOutcome {
    pub fn from_hit(hit: bool) -> Self {
        if hit {
            Self::Hit
        } else {
            Self::Miss
        }
    }
}

/// Buckets for small latencies: from 10 ns to 1 ms.
const SMALL_LATENCIES: Buckets = Buckets::values(&[
    1e-8, 2.5e-8, 5e-8, 1e-7, 2.5e-7, 5e-7, 1e-6, 2.5e-6, 5e-6, 1e-5, 2.5e-5, 5e-5, 1e-4, 1e-3,
]);

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_state_cache")]
pub(super) struct CacheMetrics {
    /// Latency of calling a cache method.
    #[metrics(buckets = SMALL_LATENCIES, labels = ["name", "method"])]
    pub latency: LabeledFamily<(&'static str, Method), Histogram<Duration>, 2>,
    /// Counter for hits / misses for a cache.
    #[metrics(labels = ["name", "kind"])]
    pub requests: LabeledFamily<(&'static str, RequestOutcome), Counter, 2>,
    /// Number of entries in the cache.
    #[metrics(labels = ["name"])]
    pub len: LabeledFamily<&'static str, Gauge<u64>>,
    /// Approximate memory usage of the cache.
    #[metrics(labels = ["name"])]
    pub used_memory: LabeledFamily<&'static str, Gauge<u64>>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<CacheMetrics> = vise::Global::new();
