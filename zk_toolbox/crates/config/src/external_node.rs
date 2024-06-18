use std::num::NonZeroUsize;

use serde::Deserialize;
use types::{ChainId, L1BatchCommitDataGeneratorMode};

use crate::{consts::EN_CONFIG_FILE, traits::FileConfigWithDefaultName};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ENConfig {
    // Genesis
    pub l2_chain_id: ChainId,
    pub l1_chain_id: ChainId,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitDataGeneratorMode,

    // Main node configuration
    pub main_node_url: String,
    pub main_node_rate_limit_rps: Option<NonZeroUsize>,
}

impl FileConfigWithDefaultName for ENConfig {
    const FILE_NAME: &'static str = EN_CONFIG_FILE;
}
