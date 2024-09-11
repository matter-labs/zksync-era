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
    // Note: `serde` isn't used with protobuf config. The same alias is implemented in
    // `zksync_protobuf_config` manually.
    // Rename is required to not introduce breaking changes in the API for existing clients.
    #[serde(
        alias = "recursion_scheduler_level_vk_hash",
        rename(serialize = "recursion_scheduler_level_vk_hash")
    )]
    pub snark_wrapper_vk_hash: H256,
    pub fee_account: Address,
    pub dummy_verifier: bool,
    pub l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
}

impl GenesisConfig {
    pub fn settlement_layer_id(&self) -> SLChainId {
        self.sl_chain_id.unwrap_or(self.l1_chain_id.into())
    }
}

impl GenesisConfig {
    pub fn for_tests() -> Self {
        GenesisConfig {
            genesis_root_hash: Some(H256::repeat_byte(0x01)),
            rollup_last_leaf_index: Some(26),
            snark_wrapper_vk_hash: H256::repeat_byte(0x02),
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

#[cfg(test)]
mod tests {
    use super::GenesisConfig;

    // This test checks that serde overrides (`rename`, `alias`) work for `snark_wrapper_vk_hash` field.
    #[test]
    fn genesis_serde_snark_wrapper_vk_hash() {
        let genesis = GenesisConfig::for_tests();
        let genesis_str = serde_json::to_string(&genesis).unwrap();

        // Check that we use backward-compatible name in serialization.
        // If you want to remove this check, make sure that all the potential clients are updated.
        assert!(
            genesis_str.contains("recursion_scheduler_level_vk_hash"),
            "Serialization should use backward-compatible name"
        );

        let genesis2: GenesisConfig = serde_json::from_str(&genesis_str).unwrap();
        assert_eq!(genesis, genesis2);

        let genesis_json = r#"{
            "snark_wrapper_vk_hash": "0x1111111111111111111111111111111111111111111111111111111111111111",
            "l1_chain_id": 1,
            "l2_chain_id": 1,
            "fee_account": "0x1111111111111111111111111111111111111111",
            "dummy_verifier": false, 
            "l1_batch_commit_data_generator_mode": "Rollup"
        }"#;
        serde_json::from_str::<GenesisConfig>(genesis_json).unwrap_or_else(|err| {
            panic!("Failed to parse genesis config with a new name: {}", err)
        });
    }
}
