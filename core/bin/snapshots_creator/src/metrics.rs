//! Metrics for the snapshot creator.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(crate) enum FactoryDepsStage {
    LoadFromPostgres,
    SaveToGcs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(crate) enum StorageChunkStage {
    LoadFromPostgres,
    SaveToGcs,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "snapshots_creator")]
pub(crate) struct SnapshotsCreatorMetrics {
    /// Number of chunks in the most recently generated snapshot. Set when a snapshot generation starts.
    pub storage_logs_chunks_count: Gauge<u64>,
    /// Number of chunks left to process for the snapshot being currently generated.
    pub storage_logs_chunks_left_to_process: Gauge<u64>,
    /// Total latency of snapshot generation.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub snapshot_generation_duration: Histogram<Duration>,
    /// L1 batch number for the most recently generated snapshot. Set *after* the snapshot
    /// is fully generated.
    pub snapshot_l1_batch: Gauge<u64>,
    /// Latency of storage log chunk processing split by stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub storage_logs_processing_duration: Family<StorageChunkStage, Histogram<Duration>>,
    /// Latency of factory deps processing split by stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub factory_deps_processing_duration: Family<FactoryDepsStage, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<SnapshotsCreatorMetrics> = vise::Global::new();
