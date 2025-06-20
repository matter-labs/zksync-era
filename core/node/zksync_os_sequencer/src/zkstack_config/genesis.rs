#![allow(dead_code)]

use serde::Deserialize;
use serde_with::{serde_as, TryFromInto};
use zksync_types::{Address, L1ChainId, L2ChainId, ProtocolVersionId, H256};

#[serde_as]
#[derive(Debug, Deserialize, Clone)]
pub struct GenesisConfig {
    pub genesis_root: H256,
    pub genesis_rollup_leaf_index: u64,
    pub genesis_batch_commitment: H256,
    #[serde_as(as = "TryFromInto<u16>")]
    pub genesis_protocol_version: ProtocolVersionId,
    pub default_aa_hash: H256,
    pub bootloader_hash: H256,
    pub evm_emulator_hash: Option<H256>,
    pub l1_chain_id: L1ChainId,
    pub l2_chain_id: L2ChainId,
    pub fee_account: Address,
}
