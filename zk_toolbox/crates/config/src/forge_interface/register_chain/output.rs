use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::{ReadConfig, SaveConfig};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RegisterChainOutput {
    pub diamond_proxy_addr: Address,
    pub governance_addr: Address,
}

impl ReadConfig for RegisterChainOutput {}
impl SaveConfig for RegisterChainOutput {}
