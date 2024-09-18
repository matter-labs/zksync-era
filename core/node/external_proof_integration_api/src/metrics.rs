use std::time::Duration;

use vise::{EncodeLabelSet, EncodeLabelValue, Histogram, LabeledFamily, Metrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "outcome", rename_all = "snake_case")]
pub(crate) enum CallOutcome {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "type", rename_all = "snake_case")]
pub(crate) enum Method {
    GetLatestProofGenerationData,
    GetSpecificProofGenerationData,
    VerifyProof,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "external_proof_integration_api")]
pub(crate) struct ProofIntegrationApiMetrics {
    #[metrics(labels = ["method", "outcome"], buckets = vise::Buckets::LATENCIES)]
    pub call_latency: LabeledFamily<(Method, CallOutcome), Histogram<Duration>, 2>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<ProofIntegrationApiMetrics> = vise::Global::new();
