//! General-purpose cache metrics.

use std::time::Duration;

use vise::{
    Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Gauge, Histogram, Info, LabeledFamily,
    Metrics, MetricsFamily, Unit,
};

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

#[derive(Debug, EncodeLabelSet)]
pub(super) struct LruCacheConfig {
    /// Cache capacity in bytes.
    #[metrics(unit = Unit::Bytes)]
    pub capacity: u64,
}

#[derive(Debug, EncodeLabelSet)]
pub(super) struct SequentialCacheConfig {
    /// Cache capacity in number of items.
    pub capacity: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(super) struct CacheLabels {
    name: &'static str,
}

impl From<&'static str> for CacheLabels {
    fn from(name: &'static str) -> Self {
        Self { name }
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_state_cache")]
pub(super) struct CacheMetrics {
    /// Configuration of LRU caches.
    pub lru_info: Info<LruCacheConfig>,
    /// Configuration of sequential caches.
    pub sequential_info: Info<SequentialCacheConfig>,

    /// Latency of calling a cache method.
    #[metrics(buckets = SMALL_LATENCIES, labels = ["method"])]
    pub latency: LabeledFamily<Method, Histogram<Duration>>,
    /// Counter for hits / misses for a cache.
    #[metrics(labels = ["kind"])]
    pub requests: LabeledFamily<RequestOutcome, Counter>,
    /// Number of entries in the cache.
    pub len: Gauge<u64>,
    /// Approximate memory usage of the cache.
    pub used_memory: Gauge<u64>,
}

#[vise::register]
pub(super) static METRICS: MetricsFamily<CacheLabels, CacheMetrics> = MetricsFamily::new();
