use ethers::types::{Address, H256};
use serde::{Deserialize, Serialize};
use types::{ChainId, L1BatchCommitDataGeneratorMode};

use crate::{
    consts::GENESIS_FILE,
    traits::{
        PathWithBasePath, ReadConfig, ReadConfigWithBasePath, SaveConfig, SaveConfigWithBasePath,
    },
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GenesisConfig {
    pub l2_chain_id: ChainId,
    pub l1_chain_id: u32,
    pub l1_batch_commit_data_generator_mode: Option<L1BatchCommitDataGeneratorMode>,
    pub bootloader_hash: H256,
    pub default_aa_hash: H256,
    pub fee_account: Address,
    pub genesis_batch_commitment: H256,
    pub genesis_rollup_leaf_index: u32,
    pub genesis_root: H256,
    pub genesis_protocol_version: u64,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

impl PathWithBasePath for GenesisConfig {
    const FILE_NAME: &'static str = GENESIS_FILE;
}

impl ReadConfig for GenesisConfig {}
impl ReadConfigWithBasePath for GenesisConfig {}
impl SaveConfig for GenesisConfig {}
impl SaveConfigWithBasePath for GenesisConfig {}
