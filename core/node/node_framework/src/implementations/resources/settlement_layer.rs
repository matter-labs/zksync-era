use zksync_types::settlement::SettlementMode;

use crate::Resource;

#[derive(Debug, Clone)]
pub struct SettlementModeResource(pub SettlementMode);

impl Resource for SettlementModeResource {
    fn name() -> String {
        "common/settlement_mode".into()
    }
}
