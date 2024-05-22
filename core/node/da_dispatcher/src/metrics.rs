use std::time::Duration;

use vise::{Buckets, Gauge, Histogram, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_da_dispatcher")]
pub(super) struct DataAvailabilityDispatcherMetrics {
    /// Latency of the dispatch of the blob.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub blob_dispatch_latency: Histogram<Duration>,
    /// The duration between the moment when the blob is dispatched and the moment when it is included.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub inclusion_latency: Histogram<Duration>,
    /// Size of the dispatched blob.
    #[metrics(buckets = Buckets::exponential(1_024.0..=16.0 * 1_024.0 * 1_024.0, 2.0))]
    pub blob_size: Histogram<usize>,

    /// Number of transactions resent by the Ethereum sender.
    #[metrics(buckets = Buckets::linear(0.0..=10.0, 1.0))]
    pub dispatch_call_retries: Histogram<usize>,
    /// Last L1 batch number observed by the DA dispatcher.
    pub last_known_l1_batch: Gauge<usize>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<DataAvailabilityDispatcherMetrics> = vise::Global::new();
