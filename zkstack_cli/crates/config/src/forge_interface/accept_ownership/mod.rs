use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::ZkStackConfigTrait;

impl ZkStackConfigTrait for AcceptOwnershipInput {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AcceptOwnershipInput {
    pub target_addr: Address,
    pub governor: Address,
}
