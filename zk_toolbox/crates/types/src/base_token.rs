use ethers::types::Address;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BaseToken {
    pub address: Address,
    pub nominator: u64,
    pub denominator: u64,
}

impl BaseToken {
    #[must_use]
    pub fn eth() -> Self {
        Self {
            nominator: 1,
            denominator: 1,
            address: Address::from_low_u64_be(1),
        }
    }
}
