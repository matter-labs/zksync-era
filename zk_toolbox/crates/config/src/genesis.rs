use crate::traits::ReadConfig;
use crate::traits::SaveConfig;
use crate::ChainId;
use crate::L1BatchCommitDataGeneratorMode;
use alloy_primitives::Address;
use alloy_primitives::B256;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GenesisConfig {
    pub l2_chain_id: ChainId,
    pub l1_chain_id: u32,
    pub l1_batch_commit_data_generator_mode: Option<L1BatchCommitDataGeneratorMode>,
    pub bootloader_hash: B256,
    pub default_aa_hash: B256,
    pub fee_account: Address,
    pub genesis_batch_commitment: B256,
    pub genesis_rollup_leaf_index: u32,
    pub genesis_root: B256,
    pub genesis_protocol_version: u64,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

impl ReadConfig for GenesisConfig {}
impl SaveConfig for GenesisConfig {}
