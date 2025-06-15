use std::time::Duration;
use vise::{Histogram, LabeledFamily, EncodeLabelSet, Family, Gauge, Metrics, Unit, Buckets, Counter};
use zksync_types::prover_dal::WitnessJobStatusFailed;

const LATENCIES_FAST: Buckets = Buckets::exponential(0.0000001..=1.0, 2.0);

#[derive(Debug, Metrics)]
pub struct ApiMetrics {
    // todo: maybe split off performance metrics into a separate struct?
    #[metrics(unit = Unit::Seconds, labels = ["method"], buckets = LATENCIES_FAST)]
    pub response_time: LabeledFamily<&'static str, Histogram<Duration>>,
}
#[vise::register]
pub(crate) static API_METRICS: vise::Global<ApiMetrics> = vise::Global::new();