use serde::{Deserialize, Serialize};
use zksync_basic_types::{Address, L1ChainId, L2ChainId, H256};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct GenesisConfig {
    pub protocol_version: u16,
    pub genesis_root_hash: H256,
    pub rollup_last_leaf_index: u64,
    pub genesis_commitment: H256,
    pub bootloader_hash: H256,
    pub default_aa_hash: H256,
    pub fee_account: Address,
    pub l1_chain_id: L1ChainId,
    pub l2_chain_id: L2ChainId,
    pub recursion_node_level_vk_hash: H256,
    pub recursion_leaf_level_vk_hash: H256,
    pub recursion_circuits_set_vks_hash: H256,
    pub recursion_scheduler_level_vk_hash: H256,
}
