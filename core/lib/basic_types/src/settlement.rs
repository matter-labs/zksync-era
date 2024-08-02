use serde::{Deserialize, Serialize};

/// An enum which is used to describe whether a zkSync network settles to L1 or to the gateway.
/// Gateway is an Ethereum-compatible L2 and so it requires different treatment with regards to DA handling.
#[derive(Default, Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SettlementMode {
    #[default]
    SettlesToL1,
    Gateway,
}

impl SettlementMode {
    pub fn is_gateway(&self) -> bool {
        matches!(self, Self::Gateway)
    }
}
