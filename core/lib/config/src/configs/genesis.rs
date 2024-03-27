use serde::{Deserialize, Serialize};
use zksync_basic_types::{Address, L1ChainId, L2ChainId, H256};

/// This config represents the genesis state of the chain.
/// Each chain has this config immutable and we update it only during the protocol upgrade
#[derive(Debug, Serialize, Deserialize, Clone, PartialOrd, PartialEq)]
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
    pub recursion_scheduler_level_vk_hash: H256,
}
