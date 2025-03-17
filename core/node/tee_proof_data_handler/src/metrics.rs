use std::{fmt, time::Duration};

use vise::{EncodeLabelSet, EncodeLabelValue, Family, Histogram, LabeledFamily, Metrics, Unit};
use zksync_object_store::bincode;
use zksync_prover_interface::inputs::WitnessInputData;
use zksync_types::tee_types::TeeType;

const BYTES_IN_MEGABYTE: u64 = 1024 * 1024;

#[derive(Debug, Metrics)]
pub(super) struct ProofDataHandlerMetrics {
    #[metrics(buckets = vise::Buckets::LATENCIES, unit = Unit::Seconds)]
    pub tee_proof_roundtrip_time: Family<MetricsTeeType, Histogram<Duration>>,
    #[metrics(labels = ["method", "status"], buckets = vise::Buckets::LATENCIES)]
    pub call_latency: LabeledFamily<(Method, u16), Histogram<Duration>, 2>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "type", rename_all = "snake_case")]
pub(crate) enum Method {
    GetTeeProofInputs,
    TeeSubmitProofs,
    TeeRegisterAttestation,
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
