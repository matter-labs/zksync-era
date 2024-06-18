use alloy::primitives::{Address, B256};
use serde::{Deserialize, Serialize};
use types::{ChainId, L1BatchCommitDataGeneratorMode, ProtocolSemanticVersion};

use crate::{consts::GENESIS_FILE, traits::FileConfigWithDefaultName};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GenesisConfig {
    pub l2_chain_id: ChainId,
    pub l1_chain_id: u64,
    pub l1_batch_commit_data_generator_mode: Option<L1BatchCommitDataGeneratorMode>,
    pub bootloader_hash: B256,
    pub default_aa_hash: B256,
    pub fee_account: Address,
    pub genesis_batch_commitment: B256,
    pub genesis_rollup_leaf_index: u32,
    pub genesis_root: B256,
    pub genesis_protocol_version: u64,
    pub genesis_protocol_semantic_version: ProtocolSemanticVersion,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

impl FileConfigWithDefaultName for GenesisConfig {
    const FILE_NAME: &'static str = GENESIS_FILE;
}
