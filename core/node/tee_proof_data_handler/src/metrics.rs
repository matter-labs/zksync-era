use std::{fmt, time::Duration};

use vise::{EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics, Unit};
use zksync_types::tee_types::TeeType;

#[derive(Debug, Metrics)]
pub(super) struct TeeProofDataHandlerMetrics {
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
pub(super) static METRICS: vise::Global<TeeProofDataHandlerMetrics> = vise::Global::new();
