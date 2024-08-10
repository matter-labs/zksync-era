use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::ZkToolboxConfig;

impl ZkToolboxConfig for InitializeBridgeOutput {}

impl ZkToolboxConfig for DefaultL2UpgradeOutput {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeBridgeOutput {
    pub l2_shared_bridge_proxy: Address,
    // TODO: move it out into a different script.
    pub l2_da_validator_addr: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultL2UpgradeOutput {
    pub l2_default_upgrader: Address,
}
