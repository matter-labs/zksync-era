use std::time::Duration;

use vise::{Buckets, Gauge, Histogram, Metrics, Unit};

/// Buckets for `blob_dispatch_latency` (from 0.1 to 120 seconds).
const DISPATCH_LATENCIES: Buckets =
    Buckets::values(&[0.1, 0.5, 1.0, 2.0, 5.0, 10.0, 30.0, 60.0, 120.0, 240.0]);

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_da_dispatcher")]
pub(super) struct DataAvailabilityDispatcherMetrics {
    /// Latency of the dispatch of the blob. Only the communication with DA layer.
    #[metrics(buckets = DISPATCH_LATENCIES, unit = Unit::Seconds)]
    pub blob_dispatch_latency: Histogram<Duration>,
    /// The duration between the moment when the blob is dispatched and the moment when it is included.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub inclusion_latency: Histogram<Duration>,
    /// Size of the dispatched blob.
    /// Buckets are bytes ranging from 1 KB to 16 MB, which has to satisfy all blob size values.
    #[metrics(buckets = Buckets::exponential(1_024.0..=16.0 * 1_024.0 * 1_024.0, 2.0), unit = Unit::Bytes)]
    pub blob_size: Histogram<usize>,
    /// Number of transactions resent by the DA dispatcher.
    #[metrics(buckets = Buckets::linear(0.0..=10.0, 1.0))]
    pub dispatch_call_retries: Histogram<usize>,
    /// Last L1 batch that was dispatched to the DA layer.
    pub last_dispatched_l1_batch: Gauge<usize>,
    /// Last L1 batch that has its inclusion finalized by DA layer.
    pub last_included_l1_batch: Gauge<usize>,
    /// The delay between the moment batch was sealed and the moment it was dispatched. Includes
    /// both communication with DA layer and time it spends in the queue on the `da_dispatcher` side.
    #[metrics(buckets = DISPATCH_LATENCIES, unit = Unit::Seconds)]
    pub sealed_to_dispatched_lag: Histogram<Duration>,
    /// The balance of the operator wallet on DA network.
    pub operator_balance: Gauge<u64>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<DataAvailabilityDispatcherMetrics> = vise::Global::new();
