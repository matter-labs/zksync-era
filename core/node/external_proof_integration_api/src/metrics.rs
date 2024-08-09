use std::time::Duration;

use tokio::time::Instant;
use vise::{Counter, EncodeLabelSet, EncodeLabelValue, Family, Histogram, LabeledFamily, Metrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "outcome", rename_all = "snake_case")]
pub(crate) enum CallOutcome {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "type", rename_all = "snake_case")]
pub(crate) enum MethodType {
    GetLatestProofGenerationData,
    GetSpecificProofGenerationData,
    VerifyProof,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "external_proof_integration_api")]
pub(crate) struct ProofIntegrationApiMetrics {
    pub call_count: Family<MethodType, Counter<u64>>,
    #[metrics(labels = ["type", "outcome"])]
    pub call_outcome: LabeledFamily<(MethodType, CallOutcome), Counter<u64>, 2>,
    #[metrics(buckets = vise::Buckets::LATENCIES)]
    pub call_latency: Family<MethodType, Histogram<Duration>>,
}

pub(crate) struct MethodCallGuard {
    method_type: MethodType,
    outcome: CallOutcome,
    started_at: Instant,
}

impl MethodCallGuard {
    pub(crate) fn new(method_type: MethodType) -> Self {
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
        METRICS.call_outcome[&(self.method_type, self.outcome)].inc();
        METRICS.call_count[&self.method_type].inc();
        METRICS.call_latency[&self.method_type].observe(self.started_at.elapsed());
    }
}

#[vise::register]
pub(crate) static METRICS: vise::Global<ProofIntegrationApiMetrics> = vise::Global::new();
