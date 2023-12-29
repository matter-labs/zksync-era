use serde::{Deserialize, Serialize};
use serde_with::DeserializeFromStr;
use zksync_basic_types::U256;
use zksync_system_constants::MAX_GAS_PER_PUBDATA_BYTE;
use zksync_utils::ceil_div;

/// Output to be provided into the VM
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeModelOutput {
    /// Fair L2 gas price to provide
    pub fair_l2_gas_price: u64,
    /// Fair pubdata price to provide.
    /// In this version, it MUST be equal to 17 * l1_gas_price
    pub fair_pubdata_price: u64,

    /// The L1 gas price to provide to the VM
    pub l1_gas_price: u64,
}
