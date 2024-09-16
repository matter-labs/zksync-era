use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics, Unit};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "stage", rename_all = "snake_case")]
pub(super) enum SnapshotStage {
    BatchHeader,
    ProtectiveReads,
    TouchedSlots,
    PreviousValues,
    InitialWrites,
    Bytecodes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "kind", rename_all = "snake_case")]
pub(super) enum AccessKind {
    ReadValue,
    IsWriteInitial,
    LoadFactoryDep,
    GetEnumerationIndex,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "state_snapshot")]
pub(super) struct SnapshotMetrics {
    /// Latency of loading a batch snapshot split by stage.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub load_latency: Family<SnapshotStage, Histogram<Duration>>,
    /// Latency of accessing the fallback storage for a batch snapshot.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub fallback_access_latency: Family<AccessKind, Histogram<Duration>>,
}

#[vise::register]
pub(super) static SNAPSHOT_METRICS: vise::Global<SnapshotMetrics> = vise::Global::new();
