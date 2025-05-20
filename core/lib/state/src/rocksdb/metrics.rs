//! Metrics for `RocksdbStorage`.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics, Unit};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum RecoveryStage {
    LoadFactoryDeps,
    SaveFactoryDeps,
    LoadChunkStarts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum ChunkRecoveryStage {
    AcquireConnection,
    LoadEntries,
    LockDb,
    SaveEntries,
}

/// Recovery-related group of metrics.
#[derive(Debug, Metrics)]
#[metrics(prefix = "server_state_keeper_secondary_storage_recovery")]
pub(super) struct RocksdbRecoveryMetrics {
    /// Number of chunks recovered.
    pub recovered_chunk_count: Gauge<usize>,
    /// Latency of a storage recovery stage (not related to the recovery of a particular chunk;
    /// those metrics are tracked in the `chunk_latency` histogram).
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub latency: Family<RecoveryStage, Histogram<Duration>>,
    /// Latency of a chunk recovery stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub chunk_latency: Family<ChunkRecoveryStage, Histogram<Duration>>,
}

#[vise::register]
pub(super) static RECOVERY_METRICS: vise::Global<RocksdbRecoveryMetrics> = vise::Global::new();
