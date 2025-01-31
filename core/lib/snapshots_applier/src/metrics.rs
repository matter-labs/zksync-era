//! Metrics for the snapshot applier.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(crate) enum StorageLogsChunksStage {
    LoadFromGcs,
    SaveToPostgres,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(crate) enum InitialStage {
    FetchMetadataFromMainNode,
    ApplyFactoryDeps,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "snapshots_applier")]
pub(crate) struct SnapshotsApplierMetrics {
    /// Number of chunks in the applied snapshot. Set when snapshots applier starts.
    pub storage_logs_chunks_count: Gauge<usize>,

    /// Number of chunks left to apply.
    pub storage_logs_chunks_left_to_process: Gauge<usize>,

    /// Total latency of applying snapshot.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub snapshot_applying_duration: Histogram<Duration>,

    /// Latency of initial recovery operation split by stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub initial_stage_duration: Family<InitialStage, Histogram<Duration>>,

    /// Latency of storage log chunk processing split by stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub storage_logs_chunks_duration: Family<StorageLogsChunksStage, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<SnapshotsApplierMetrics> = vise::Global::new();
