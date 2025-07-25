use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Gauge, Histogram, Metrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "operation_result", rename_all = "snake_case")]
pub(super) enum OperationResult {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct OperationResultLabels {
    pub result: OperationResult,
}

#[derive(Debug, Metrics)]
#[metrics(prefix = "base_token_adjuster")]
pub(crate) struct BaseTokenAdjusterMetrics {
    pub l1_gas_used: Gauge<u64>,
    pub ratio: Gauge<f64>,
    pub ratio_l1: Gauge<f64>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub external_price_api_latency: Family<OperationResultLabels, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub l1_update_latency: Family<OperationResultLabels, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<BaseTokenAdjusterMetrics> = vise::Global::new();
