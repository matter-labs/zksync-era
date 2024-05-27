use crate::traits::{ReadConfig, SaveConfig};
use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RegisterChainOutput {
    pub diamond_proxy_addr: Address,
    pub governance_addr: Address,
}

impl ReadConfig for RegisterChainOutput {}
impl SaveConfig for RegisterChainOutput {}
