use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::FileConfigTrait;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RegisterChainOutput {
    pub diamond_proxy_addr: Address,
    pub governance_addr: Address,
    pub chain_admin_addr: Address,
    pub l2_legacy_shared_bridge_addr: Option<Address>,
    pub access_control_restriction_addr: Address,
    pub chain_proxy_admin_addr: Address,
}

impl FileConfigTrait for RegisterChainOutput {}
