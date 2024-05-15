use ethers::addressbook::Address;
use serde::{Deserialize, Serialize};

use crate::configs::{ReadConfig, SaveConfig};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct RegisterHyperchainOutput {
    pub diamond_proxy_addr: Address,
}

impl ReadConfig for RegisterHyperchainOutput {}
impl SaveConfig for RegisterHyperchainOutput {}
