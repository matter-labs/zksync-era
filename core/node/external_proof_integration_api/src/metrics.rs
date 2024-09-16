use std::time::Duration;

use crate::error::ProcessorError;
use vise::{Counter, EncodeLabelSet, EncodeLabelValue, Histogram, LabeledFamily, Metrics};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "reason", rename_all = "snake_case")]
pub enum FailureReason {
    Serialization,
    InvalidProof,
    BatchNotReady,
    InvalidFile,
    Internal,
    ProofIsGone,
}

impl From<&ProcessorError> for FailureReason {
    fn from(err: &ProcessorError) -> Self {
        match err {
            ProcessorError::Serialization(_) => Self::Serialization,
            ProcessorError::InvalidProof => Self::InvalidProof,
            ProcessorError::BatchNotReady(_) => Self::BatchNotReady,
            ProcessorError::InvalidFile(_) => Self::InvalidFile,
            ProcessorError::Internal => Self::Internal,
            ProcessorError::ProofIsGone => Self::ProofIsGone,
        }
    }
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "external_proof_integration_api")]
pub(crate) struct ProofIntegrationApiMetrics {
    #[metrics(labels = ["method", "outcome"], buckets = vise::Buckets::LATENCIES)]
    pub call_latency: LabeledFamily<(Method, CallOutcome), Histogram<Duration>, 2>,
    #[metrics(labels = ["method", "reason"])]
    pub failed_calls: LabeledFamily<(Method, FailureReason), Counter<u64>, 2>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<ProofIntegrationApiMetrics> = vise::Global::new();
