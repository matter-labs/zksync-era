use std::time::Duration;

use vise::{Buckets, Histogram, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "celestia")]
pub(super) struct CelestiaMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    celestia_app_submit_blob_response_time: Histogram<Duration>,
    #[metrics(buckets = Buckets::LATENCIES)]
    eq_service_response_time: Histogram<Duration>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<CelestiaMetrics> = vise::Global::new();
