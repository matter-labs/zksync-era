use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::FileConfigTrait;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeployPaymasterOutput {
    pub paymaster: Address,
}

impl FileConfigTrait for DeployPaymasterOutput {}
