//! Metrics for `RocksdbStorage`.

use vise::{Buckets, Gauge, Histogram, Metrics};

use std::time::Duration;

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_state_keeper_secondary_storage")]
pub(super) struct RocksdbStorageMetrics {
    /// Total latency of the storage update after initialization.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub update: Histogram<Duration>,
    /// Lag of the secondary storage relative to Postgres.
    pub lag: Gauge<u64>,
    /// Estimated number of entries in the secondary storage.
    pub size: Gauge<u64>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<RocksdbStorageMetrics> = vise::Global::new();
