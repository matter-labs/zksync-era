//! Gas adjuster metrics.

use vise::{Gauge, Metrics};

#[derive(Debug, Metrics)]
#[metrics(prefix = "server_gas_adjuster")]
pub(super) struct GasAdjusterMetrics {
    pub current_base_fee_per_gas: Gauge<u64>,
    pub median_base_fee_per_gas: Gauge<u64>,
}

#[vise::register]
pub(super) static METRICS: vise::Global<GasAdjusterMetrics> = vise::Global::new();
