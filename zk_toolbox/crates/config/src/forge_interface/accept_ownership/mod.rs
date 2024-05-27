use crate::traits::{ReadConfig, SaveConfig};
use alloy_primitives::Address;
use serde::{Deserialize, Serialize};

impl ReadConfig for AcceptOwnershipInput {}
impl SaveConfig for AcceptOwnershipInput {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AcceptOwnershipInput {
    pub target_addr: Address,
    pub governor: Address,
}
