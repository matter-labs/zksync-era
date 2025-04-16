use std::{fmt, time::Duration};

use vise::{EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics, Unit};
use zksync_types::tee_types::TeeType;

#[derive(Debug, Metrics)]
pub(super) struct ProofDataHandlerMetrics {
    #[metrics(buckets = vise::Buckets::exponential(1.0..=2_048.0, 2.0))]
    pub vm_run_data_blob_size_in_mb: Histogram<u64>,
    #[metrics(buckets = vise::Buckets::exponential(1.0..=2_048.0, 2.0))]
    pub merkle_paths_blob_size_in_mb: Histogram<u64>,
    #[metrics(buckets = vise::Buckets::exponential(1.0..=2_048.0, 2.0))]
    pub eip_4844_blob_size_in_mb: Histogram<u64>,
    #[metrics(buckets = vise::Buckets::exponential(1.0..=2_048.0, 2.0))]
    pub total_blob_size_in_mb: Histogram<u64>,
    #[metrics(buckets = vise::Buckets::LATENCIES, unit = Unit::Seconds)]
    pub tee_proof_roundtrip_time: Family<MetricsTeeType, Histogram<Duration>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "tee_type")]
pub(crate) struct MetricsTeeType(pub TeeType);

impl fmt::Display for MetricsTeeType {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

impl From<TeeType> for MetricsTeeType {
    fn from(value: TeeType) -> Self {
        Self(value)
    }
}

#[vise::register]
pub(super) static METRICS: vise::Global<ProofDataHandlerMetrics> = vise::Global::new();
