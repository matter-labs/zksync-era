//! Metrics for `PostgresStorage`.

use vise::{Buckets, Counter, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics};

use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum ValuesUpdateStage {
    LoadKeys,
    RemoveStaleKeys,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_state_cache")]
pub(super) struct ValuesCacheMetrics {
    /// Number of times the values cache moved forward when we attempted to insert a value into it.
    pub stale_values: Counter,
    /// Number of times the values cache was emptied because it was too far back.
    pub values_emptied: Counter,
    /// Latency of values cache update stages.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub values_update: Family<ValuesUpdateStage, Histogram<Duration>>,
    /// Number of keys modified during a specific values cache update.
    #[metrics(buckets = &[10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1_000.0])]
    pub values_update_modified_keys: Histogram<usize>,
    /// Current miniblock for the values cache.
    pub values_valid_for_miniblock: Gauge<u64>,
    /// Number of times the negative initial writes cache was successfully used. This is distinct
    /// from cache hits (we can hit the cache, but the cached value may be outdated).
    pub effective_values: Counter,
}

#[vise::register]
pub(super) static CACHE_METRICS: vise::Global<ValuesCacheMetrics> = vise::Global::new();

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "method", rename_all = "snake_case")]
pub(super) enum Method {
    ReadValue,
    IsWriteInitial,
    LoadFactoryDep,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "state_postgres")]
pub(super) struct PostgresStorageMetrics {
    /// Latency of storage reading methods for Postgres-backed storage.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub storage: Family<Method, Histogram<Duration>>,
}

#[vise::register]
pub(super) static STORAGE_METRICS: vise::Global<PostgresStorageMetrics> = vise::Global::new();
