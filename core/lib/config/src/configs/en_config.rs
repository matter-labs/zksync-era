use std::num::{NonZeroU64, NonZeroUsize};

use serde::Deserialize;
use zksync_basic_types::{url::SensitiveUrl, L1ChainId, L2ChainId, SLChainId};

/// Temporary config for initializing external node, will be completely replaced by consensus config later
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ENConfig {
    // Genesis
    pub l2_chain_id: L2ChainId,
    pub l1_chain_id: L1ChainId,

    // Main node configuration
    pub main_node_url: SensitiveUrl,
    pub main_node_rate_limit_rps: Option<NonZeroUsize>,

    pub bridge_addresses_refresh_interval_sec: Option<NonZeroU64>,

    pub gateway_chain_id: Option<SLChainId>,
}
