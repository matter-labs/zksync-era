//! Metrics for contract verification.

use std::time::Duration;

use vise::{Buckets, Histogram, LabeledFamily, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "api_contract_verification")]
pub(super) struct ContractVerificationMetrics {
    #[metrics(buckets = Buckets::LATENCIES, labels = ["method"])]
    pub call: LabeledFamily<&'static str, Histogram<Duration>>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<ContractVerificationMetrics> = vise::Global::new();
