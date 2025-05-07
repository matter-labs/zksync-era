use zksync_config::configs::eth_sender::SenderConfig;
use zksync_gateway_migrator::WorkingSettlementLayer;
use zksync_types::settlement::SettlementLayer;

use crate::Resource;

#[derive(Debug, Clone)]
pub struct SettlementModeResource(WorkingSettlementLayer);

impl SettlementModeResource {
    pub fn new(working_settlement_layer: WorkingSettlementLayer) -> Self {
        Self(working_settlement_layer)
    }

    /// For configuring the system please use this method. It returns the settlement layer that system should assume to use
    pub fn settlement_layer(&self) -> SettlementLayer {
        self.0.settlement_layer()
    }

    /// Method returns the settlement layer, where we really should send the transactions, it could be None during the migration. Please use this method only if you know how to handle unknown settlement layer
    pub fn settlement_layer_for_sending_txs(&self) -> Option<SettlementLayer> {
        self.0.settlement_layer_for_sending_txs()
    }
}

impl Resource for SettlementModeResource {
    fn name() -> String {
        "common/settlement_mode".into()
    }
}

impl Resource for SenderConfig {
    fn name() -> String {
        "common/eth_sender_config".into()
    }
}
