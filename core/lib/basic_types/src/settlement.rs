use serde::{Deserialize, Serialize};

use crate::SLChainId;

/// An enum which is used to describe whether a zkSync network settles to L1 or to the gateway.
/// Gateway is an Ethereum-compatible L2 and so it requires different treatment with regards to DA handling.
#[derive(Debug, Copy, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", content = "chain_id")]
pub enum SettlementLayer {
    L1(SLChainId),
    Gateway(SLChainId),
}

impl SettlementLayer {
    pub fn is_gateway(self) -> bool {
        matches!(self, Self::Gateway(_))
    }
    pub fn chain_id(&self) -> SLChainId {
        match self {
            Self::L1(chain_id) | Self::Gateway(chain_id) => *chain_id,
        }
    }
    pub fn for_tests() -> Self {
        // 9 is a common chain id for localhost
        Self::L1(SLChainId(9))
    }
}
