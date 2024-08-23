use std::time::Duration;

use vise::{Buckets, EncodeLabelSet, EncodeLabelValue, Family, Histogram, Metrics};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet, EncodeLabelValue)]
#[metrics(label = "operation_result", rename_all = "snake_case")]
pub(super) enum OperationResult {
    Success,
    Failure,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EncodeLabelSet)]
pub(crate) struct OperationResultLabels {
    pub result: OperationResult,
    pub attempts: u32,
}

/// Roughly exponential buckets for gas (10k – 50M).
const GAS_BUCKETS: Buckets =
    Buckets::values(&[1e4, 2e4, 5e4, 1e5, 2e5, 5e5, 1e6, 2e6, 5e6, 1e7, 2e7, 5e7]);

#[derive(Debug, Metrics)]
#[metrics(prefix = "snapshots_creator")]
pub(crate) struct BaseTokenAdjusterMetrics {
    #[metrics(buckets = GAS_BUCKETS)]
    pub l1_gas_used: Histogram<f64>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub external_price_api_latency: Family<OperationResultLabels, Histogram<Duration>>,
    #[metrics(buckets = Buckets::LATENCIES)]
    pub l1_update_latency: Family<OperationResultLabels, Histogram<Duration>>,
}

#[vise::register]
pub(crate) static METRICS: vise::Global<BaseTokenAdjusterMetrics> = vise::Global::new();
