use zksync_config::configs::eth_sender::SenderConfig;
use zksync_node_framework::Resource;

pub use self::{aggregator::EthTxAggregatorLayer, manager::EthTxManagerLayer};

mod aggregator;
mod manager;

// FIXME: probably out of place here
#[derive(Debug, Clone)]
pub struct SenderConfigResource(pub SenderConfig);

impl Resource for SenderConfigResource {
    fn name() -> String {
        "common/eth_sender_config".into()
    }
}
