//! Metrics for the object storage.

use vise::{Buckets, Histogram, LabeledFamily, LatencyObserver, Metrics};

use std::time::Duration;

use crate::Bucket;

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_object_store")]
pub(crate) struct GcsMetrics {
    /// Latency to fetch an object from GCS.
    #[metrics(buckets = Buckets::LATENCIES, labels = ["bucket"])]
    fetching_time: LabeledFamily<&'static str, Histogram<Duration>>,
    /// Latency to store an object in GCS.
    #[metrics(buckets = Buckets::LATENCIES, labels = ["bucket"])]
    storing_time: LabeledFamily<&'static str, Histogram<Duration>>,
}

impl GcsMetrics {
    pub fn start_fetch(&self, bucket: Bucket) -> LatencyObserver<'_> {
        self.fetching_time[&bucket.as_str()].start()
    }

    pub fn start_store(&self, bucket: Bucket) -> LatencyObserver<'_> {
        self.storing_time[&bucket.as_str()].start()
    }
}

#[vise::register]
pub(crate) static GCS_METRICS: vise::Global<GcsMetrics> = vise::Global::new();
