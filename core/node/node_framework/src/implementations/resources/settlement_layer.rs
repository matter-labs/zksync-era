use zksync_config::configs::eth_sender::SenderConfig;
use zksync_types::settlement::SettlementLayer;

use crate::Resource;

#[derive(Debug, Clone)]
pub struct SettlementModeResource(pub SettlementLayer);

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
