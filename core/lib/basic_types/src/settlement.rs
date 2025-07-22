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

/// During the migration settlement layer could be unknown for the server.
/// In this case, we should not use it for sending transactions.
/// Meanwhile we should continue to work with the old settlement layer.
#[derive(Debug, Clone)]
pub struct WorkingSettlementLayer {
    unsafe_settlement_layer: SettlementLayer,
    migration_in_progress: bool,
}

impl WorkingSettlementLayer {
    pub fn new(unsafe_settlement_layer: SettlementLayer) -> Self {
        Self {
            unsafe_settlement_layer,
            migration_in_progress: false,
        }
    }

    pub fn set_migration_in_progress(&mut self, in_progress: bool) {
        self.migration_in_progress = in_progress;
    }

    pub fn settlement_layer(&self) -> SettlementLayer {
        self.unsafe_settlement_layer
    }

    pub fn settlement_layer_for_sending_txs(&self) -> Option<SettlementLayer> {
        if self.migration_in_progress {
            None
        } else {
            Some(self.unsafe_settlement_layer)
        }
    }
}
