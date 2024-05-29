use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::ReadConfig;

impl ReadConfig for InitializeBridgeOutput {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeBridgeOutput {
    pub l2_shared_bridge_implementation: Address,
    pub l2_shared_bridge_proxy: Address,
}
