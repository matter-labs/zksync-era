use std::time::Duration;

use axum::{extract::Request, middleware::Next, response::Response};
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
    Unknown,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "external_proof_integration_api")]
pub(crate) struct ProofIntegrationApiMetrics {
    #[metrics(labels = ["method", "outcome"], buckets = vise::Buckets::LATENCIES)]
    pub call_latency: LabeledFamily<(Method, CallOutcome), Histogram<Duration>, 2>,
}

pub(crate) async fn call_outcome_tracker(request: Request, next: Next) -> Response {
    let start = Instant::now();
    let path = request.uri().path();

    let method = if path.starts_with("/proof_generation_data") {
        if let Some(char) = path.get(22..23) {
            if char == "/" && path.get(23..).is_some() {
                Method::GetSpecificProofGenerationData
            } else {
                Method::GetLatestProofGenerationData
            }
        } else {
            Method::GetLatestProofGenerationData
        }
    } else if path.starts_with("/verify_proof/") {
        Method::VerifyProof
    } else {
        Method::Unknown
    };

    let response = next.run(request).await;

    let outcome = if response.status().is_success() {
        CallOutcome::Success
    } else {
        CallOutcome::Failure
    };

    METRICS.call_latency[&(method, outcome)].observe(start.elapsed());

    response
}

#[vise::register]
pub(crate) static METRICS: vise::Global<ProofIntegrationApiMetrics> = vise::Global::new();
