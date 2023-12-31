use serde::{Deserialize, Serialize};

/// Output to be provided into the VM
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchFeeModelInput {
    /// Fair L2 gas price to provide
    pub fair_l2_gas_price: u64,
    /// Fair pubdata price to provide.
    /// In this version, it MUST be equal to 17 * l1_gas_price
    pub fair_pubdata_price: u64,
    /// The L1 gas price to provide to the VM. It may not be used by the latest VM, but it is kept for the backward compatibility.
    pub l1_gas_price: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MainNodeFeeModelConfig {
    pub l1_gas_price_scale_factor: f64,
    pub l1_pubdata_price_scale_factor: f64,
    pub minimal_l2_gas_price: u64,
    pub compute_overhead_percent: f64,
    pub pubdata_overhead_percent: f64,
    pub batch_overhead_l1_gas: u64,
    pub max_gas_per_batch: u64,
    pub max_pubdata_per_batch: u64,
}

impl Default for MainNodeFeeModelConfig {
    /// Config with all 0s is not a valid config (since for instance having 0 max gas per batch may incur division by zero),
    /// so we implement a sensible default config here.
    fn default() -> Self {
        Self {
            // We don't scale L1 prices by default
            l1_gas_price_scale_factor: 1.0,
            l1_pubdata_price_scale_factor: 1.0,
            minimal_l2_gas_price: 100_000_000, // 0.1 gwei
            compute_overhead_percent: 0.0,
            pubdata_overhead_percent: 1.0, // We assume that all the batches are closed because of pubdata limit
            batch_overhead_l1_gas: 800_000,
            max_gas_per_batch: 250_000_000,
            max_pubdata_per_batch: 120_000_000,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MainNodeFeeParams {
    pub config: MainNodeFeeModelConfig,
    pub l1_gas_price: u64,
    pub l1_pubdata_price: u64,
}
