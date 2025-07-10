use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::ZkStackConfig;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DeployTeeInput {
    pub owner_address: Address,
}

impl DeployTeeInput {
    pub fn new(owner_address: Address) -> Self {
        Self { owner_address }
    }
}

impl ZkStackConfig for DeployTeeInput {}
