use std::time::Duration;

use tokio::time::Instant;
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

pub(crate) struct MethodCallGuard {
    method_type: Method,
    outcome: CallOutcome,
    started_at: Instant,
}

impl MethodCallGuard {
    pub(crate) fn new(method_type: Method) -> Self {
        MethodCallGuard {
            method_type,
            outcome: CallOutcome::Failure,
            started_at: Instant::now(),
        }
    }

    pub(crate) fn mark_successful(&mut self) {
        self.outcome = CallOutcome::Success;
    }
}

impl Drop for MethodCallGuard {
    fn drop(&mut self) {
        METRICS.call_latency[&(self.method_type, self.outcome)].observe(self.started_at.elapsed());
    }
}

#[vise::register]
pub(crate) static METRICS: vise::Global<ProofIntegrationApiMetrics> = vise::Global::new();
