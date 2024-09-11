use std::time::Duration;

use vise::{Buckets, Histogram, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_contract_verifier")]
pub(crate) struct ApiContractVerifierMetrics {
    #[metrics(buckets = Buckets::LATENCIES)]
    pub request_processing_time: Histogram<Duration>,
}

#[vise::register]
pub(crate) static API_CONTRACT_VERIFIER_METRICS: vise::Global<ApiContractVerifierMetrics> =
    vise::Global::new();
