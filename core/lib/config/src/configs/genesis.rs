use serde::{Deserialize, Serialize};
use zksync_basic_types::{
    commitment::L1BatchCommitmentMode,
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId},
    Address, L1ChainId, L2ChainId, SLChainId, H256,
};

/// This config represents the genesis state of the chain.
/// Each chain has this config immutable and we update it only during the protocol upgrade
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct GenesisConfig {
    // TODO make fields non optional, once we fully moved to file based configs.
    // Now for backward compatibility we keep it optional
    pub protocol_version: Option<ProtocolSemanticVersion>,
    pub genesis_root_hash: Option<H256>,
    pub rollup_last_leaf_index: Option<u64>,
    pub genesis_commitment: Option<H256>,
    pub bootloader_hash: Option<H256>,
    pub default_aa_hash: Option<H256>,
    pub l1_chain_id: L1ChainId,
    pub sl_chain_id: Option<SLChainId>,
    pub l2_chain_id: L2ChainId,
    pub recursion_scheduler_level_vk_hash: H256,
    pub fee_account: Address,
    pub dummy_verifier: bool,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
}

impl GenesisConfig {
    pub fn get_settlement_layer_id(&self) -> SLChainId {
        self.sl_chain_id.unwrap_or(self.l1_chain_id.into())
    }
}

impl GenesisConfig {
    pub fn for_tests() -> Self {
        GenesisConfig {
            genesis_root_hash: Some(H256::repeat_byte(0x01)),
            rollup_last_leaf_index: Some(26),
            recursion_scheduler_level_vk_hash: H256::repeat_byte(0x02),
            fee_account: Default::default(),
            genesis_commitment: Some(H256::repeat_byte(0x17)),
            bootloader_hash: Default::default(),
            default_aa_hash: Default::default(),
            l1_chain_id: L1ChainId(9),
            sl_chain_id: None,
            protocol_version: Some(ProtocolSemanticVersion {
                minor: ProtocolVersionId::latest(),
                patch: 0.into(),
            }),
            l2_chain_id: L2ChainId::default(),
            dummy_verifier: false,
            l1_batch_commit_data_generator_mode: L1BatchCommitmentMode::Rollup,
        }
    }
}
