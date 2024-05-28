use ethers::prelude::Address;
use serde::{Deserialize, Serialize};

use crate::configs::{ReadConfig, SaveConfig};

impl ReadConfig for AcceptOwnershipInput {}
impl SaveConfig for AcceptOwnershipInput {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AcceptOwnershipInput {
    pub target_addr: Address,
    pub governor: Address,
}
