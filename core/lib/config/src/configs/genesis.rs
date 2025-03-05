use zksync_basic_types::{
    commitment::L1BatchCommitmentMode,
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId},
    Address, L1ChainId, L2ChainId, H256,
};

/// This config represents the genesis state of the chain.
/// Each chain has this config immutable and we update it only during the protocol upgrade
#[derive(Debug, Clone, PartialEq)]
pub struct GenesisConfig {
    // TODO make fields non optional, once we fully moved to file based configs.
    // Now for backward compatibility we keep it optional
    pub protocol_version: Option<ProtocolSemanticVersion>,
    pub genesis_root_hash: Option<H256>,
    pub rollup_last_leaf_index: Option<u64>,
    pub genesis_commitment: Option<H256>,
    pub bootloader_hash: Option<H256>,
    pub default_aa_hash: Option<H256>,
    pub evm_emulator_hash: Option<H256>,
    pub l1_chain_id: L1ChainId,
    pub l2_chain_id: L2ChainId,
    pub snark_wrapper_vk_hash: H256,
    pub fflonk_snark_wrapper_vk_hash: Option<H256>,
    pub fee_account: Address,
    pub dummy_verifier: bool,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    pub custom_genesis_state_path: Option<String>,
}

impl GenesisConfig {
    pub fn for_tests() -> Self {
        GenesisConfig {
            genesis_root_hash: Some(H256::repeat_byte(0x01)),
            rollup_last_leaf_index: Some(26),
            snark_wrapper_vk_hash: H256::repeat_byte(0x02),
            fflonk_snark_wrapper_vk_hash: Default::default(),
            fee_account: Default::default(),
            genesis_commitment: Some(H256::repeat_byte(0x17)),
            bootloader_hash: Default::default(),
            default_aa_hash: Default::default(),
            evm_emulator_hash: Default::default(),
            l1_chain_id: L1ChainId(9),
            protocol_version: Some(ProtocolSemanticVersion {
                minor: ProtocolVersionId::latest(),
                patch: 0.into(),
            }),
            l2_chain_id: L2ChainId::default(),
            dummy_verifier: false,
            l1_batch_commit_data_generator_mode: L1BatchCommitmentMode::Rollup,
            custom_genesis_state_path: None,
        }
    }
}
