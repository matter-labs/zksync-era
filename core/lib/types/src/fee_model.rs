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
