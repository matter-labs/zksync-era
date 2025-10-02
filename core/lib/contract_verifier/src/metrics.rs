use std::time::Duration;

use vise::{Buckets, Counter, Gauge, Histogram, LabeledFamily, Metrics};

// Starting bucket from 5 sec as there is a 5 second pause between
// the verification request and the time verification status is checked for the first time.
pub const ETHERSCAN_VERIFICATION_BUCKET: Buckets =
    Buckets::values(&[5.0, 10.0, 15.0, 20.0, 30.0, 45.0, 60.0, 90.0, 120.0]);

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_contract_verifier")]
pub(crate) struct ApiContractVerifierMetrics {
    /// Latency of processing a single request.
    #[metrics(buckets = Buckets::LATENCIES)]
    pub request_processing_time: Histogram<Duration>,
    #[metrics(buckets = ETHERSCAN_VERIFICATION_BUCKET)]
    pub etherscan_request_processing_time: Histogram<Duration>,
    #[metrics(labels = ["service_name"])]
    pub failed_verifications: LabeledFamily<&'static str, Counter, 1>,
    #[metrics(labels = ["service_name"])]
    pub successful_verifications: LabeledFamily<&'static str, Counter, 1>,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "contract_verifier")]
pub(crate) struct ContractVerifierMetrics {
    #[metrics(labels = ["service_name"])]
    pub number_of_queued_requests: LabeledFamily<&'static str, Gauge<u64>>,
}

#[vise::register]
pub(crate) static API_CONTRACT_VERIFIER_METRICS: vise::Global<ApiContractVerifierMetrics> =
    vise::Global::new();

#[vise::register]
pub(crate) static CONTRACT_VERIFIER_METRICS: vise::Global<ContractVerifierMetrics> =
    vise::Global::new();
