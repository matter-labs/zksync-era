use zksync_config::configs::eth_sender::SenderConfig;
use zksync_types::{settlement::SettlementMode, SLChainId};

use crate::Resource;

#[derive(Debug, Clone)]
pub struct SettlementModeResource(pub SettlementMode);

#[derive(Debug, Clone)]
pub struct SlChainIdResource(pub SLChainId);

impl Resource for SettlementModeResource {
    fn name() -> String {
        "common/settlement_mode".into()
    }
}

impl Resource for SlChainIdResource {
    fn name() -> String {
        "common/sl_chain_id".into()
    }
}

impl Resource for SenderConfig {
    fn name() -> String {
        "common/eth_sender_config".into()
    }
}
