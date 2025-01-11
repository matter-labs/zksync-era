use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::ZkStackConfig;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RegisterChainOutput {
    pub diamond_proxy_addr: Address,
    pub governance_addr: Address,
    pub chain_admin_addr: Address,
}

impl ZkStackConfig for RegisterChainOutput {}
