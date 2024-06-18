use alloy::primitives::Address;
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
            address: Address::new([0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]),
        }
    }
}
