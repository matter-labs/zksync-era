//! Metrics for `VmRunner`.

use std::time::Duration;

use vise::{Buckets, Gauge, Histogram, Metrics};

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
    /// Total latency of running VM on an L1 batch.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub run_vm_time: Histogram<Duration>,
    /// Total latency of handling output of an L1 batch.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub output_handle_time: Histogram<Duration>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<VmRunnerMetrics> = vise::Global::new();
