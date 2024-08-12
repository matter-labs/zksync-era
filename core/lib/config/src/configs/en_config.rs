use std::num::NonZeroUsize;

use serde::Deserialize;
use zksync_basic_types::{
    commitment::L1BatchCommitmentMode, url::SensitiveUrl, L1ChainId, L2ChainId,
};

/// Temporary config for initializing external node, will be completely replaced by consensus config later
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ENConfig {
    // Genesis
    pub l2_chain_id: L2ChainId,
    pub l1_chain_id: L1ChainId,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,

    // Main node configuration
    pub main_node_url: SensitiveUrl,
    pub main_node_rate_limit_rps: Option<NonZeroUsize>,

    // Gateway configuration
    pub gateway_url: Option<SensitiveUrl>,
}
