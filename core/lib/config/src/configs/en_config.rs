use serde::Deserialize;
use zksync_basic_types::{L1ChainId, L2ChainId};

/// Temporary config for initializing external node, will be completely replaced by consensus config later
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ENConfig {
    pub l2_chain_id: L2ChainId,
    pub l1_chain_id: L1ChainId,
    pub main_node_url: String,
}
