use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::ZkStackConfig;

impl ZkStackConfig for EnableEvmEmulatorInput {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct EnableEvmEmulatorInput {
    pub target_addr: Address,
    pub governor: Address,
}
