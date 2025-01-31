//! Metrics for `VmRunner`.

use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics, Unit};
use zksync_state::OwnedStorage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelValue, EncodeLabelSet)]
#[metrics(label = "storage", rename_all = "snake_case")]
pub(super) enum StorageKind {
    Postgres,
    Snapshot,
    Rocksdb,
    Unknown,
}

impl StorageKind {
    pub fn new(storage: &OwnedStorage) -> Self {
        match storage {
            OwnedStorage::Rocksdb(_) | OwnedStorage::RocksdbWithMemory(_) => Self::Rocksdb,
            OwnedStorage::Postgres(_) => Self::Postgres,
            OwnedStorage::Snapshot(_) => Self::Snapshot,
            OwnedStorage::Boxed(_) => Self::Unknown,
        }
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "vm_runner")]
pub(super) struct VmRunnerMetrics {
    /// Last batch that has been marked as processed.
    pub last_processed_batch: Gauge<u64>,
    /// Last batch that is ready to be processed.
    pub last_ready_batch: Gauge<u64>,
    /// Current amount of batches that are being processed.
    pub in_progress_l1_batches: Gauge<u64>,
    /// Total latency of loading an L1 batch (RocksDB mode only).
    #[metrics(buckets = Buckets::LATENCIES)]
    pub storage_load_time: Histogram<Duration>,
    /// Latency of loading data and storage for a batch, grouped by the storage kind.
    #[metrics(buckets = Buckets::LATENCIES, unit = Unit::Seconds)]
    pub data_and_storage_latency: Family<StorageKind, Histogram<Duration>>,
    /// Total latency of running VM on an L1 batch.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub run_vm_time: Histogram<Duration>,
    /// Total latency of handling output of an L1 batch.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub output_handle_time: Histogram<Duration>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<VmRunnerMetrics> = vise::Global::new();
