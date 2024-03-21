use std::time::Duration;

use vise::{Buckets, Histogram, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_mempool_cache")]
pub(super) struct MempoolCacheMetrics {
    /// Latency of mempool cache updates - the time it takes to load all the new transactions from the DB.
    /// Does not include cache update time
    #[metrics(buckets = Buckets::LATENCIES)]
    pub db_poll_latency: Histogram<Duration>,
    /// Number of transactions loaded from the DB during the last cache update
    #[metrics(buckets = Buckets::exponential(1.0..=2048.0, 2.0))]
    pub tx_batch_size: Histogram<usize>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<MempoolCacheMetrics> = vise::Global::new();
