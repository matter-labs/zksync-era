use std::time::Duration;

use vise::{Counter, EncodeLabelSet, EncodeLabelValue, Histogram, LabeledFamily, Metrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "type", rename_all = "snake_case")]
pub(crate) enum Method {
    ProofGenerationData,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "prover_fri_prover_fri_gateway")]
pub(crate) struct ProverFriGatewayMetrics {
    #[metrics(label = "status_code")]
    pub submitter_http_error: LabeledFamily<u16, Counter>,
    #[metrics(labels = ["method", "status"], buckets = vise::Buckets::LATENCIES)]
    pub call_latency: LabeledFamily<(Method, u16), Histogram<Duration>, 2>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<ProverFriGatewayMetrics> = vise::Global::new();
