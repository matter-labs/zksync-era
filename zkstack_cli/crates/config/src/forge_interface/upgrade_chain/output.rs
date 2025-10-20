use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::FileConfigTrait;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChainUpgradeOutput {
    // This should be the address that controls the current `ChainAdmin`
    // contract
    pub chain_admin_addr: Address,
    pub access_control_restriction: Address,
}

impl FileConfigTrait for ChainUpgradeOutput {}
