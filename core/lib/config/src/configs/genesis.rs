use std::{
    fs,
    io::{BufReader, BufWriter},
    path::Path,
};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use smart_config::{
    de::{DeserializeContext, DeserializeParam, WellKnown, WellKnownOption},
    metadata::{BasicTypes, ParamMetadata},
    DescribeConfig, DeserializeConfig, ErrorWithOrigin,
};
use zksync_basic_types::{
    commitment::L1BatchCommitmentMode,
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId},
    Address, L1ChainId, L2ChainId, H256,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedGenesisProverConfig {
    #[serde(alias = "recursion_scheduler_level_vk_hash")]
    snark_wrapper_vk_hash: H256,
    fflonk_snark_wrapper_vk_hash: Option<H256>,
    dummy_verifier: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PersistedGenesisConfig {
    genesis_protocol_semantic_version: Option<ProtocolSemanticVersion>,
    genesis_protocol_version: Option<u16>,
    genesis_root: H256,
    genesis_rollup_leaf_index: u64,
    genesis_batch_commitment: H256,
    bootloader_hash: H256,
    default_aa_hash: H256,
    evm_emulator_hash: Option<H256>,
    l1_chain_id: L1ChainId,
    l2_chain_id: L2ChainId,
    fee_account: Address,
    l1_batch_commit_data_generator_mode: L1BatchCommitmentMode,
    prover: PersistedGenesisProverConfig,
    custom_genesis_state_path: Option<String>,
}

/// Returns an error iff the config is incomplete.
impl TryFrom<GenesisConfig> for PersistedGenesisConfig {
    type Error = anyhow::Error;

    fn try_from(config: GenesisConfig) -> Result<Self, Self::Error> {
        Ok(Self {
            genesis_protocol_semantic_version: Some(
                config
                    .protocol_version
                    .context("missing `protocol_version`")?,
            ),
            genesis_protocol_version: None, // semantic version has precedence
            genesis_root: config
                .genesis_root_hash
                .context("missing `genesis_root_hash`")?,
            genesis_rollup_leaf_index: config
                .rollup_last_leaf_index
                .context("missing `rollup_last_leaf_index`")?,
            genesis_batch_commitment: config
                .genesis_commitment
                .context("missing `genesis_commitment`")?,
            bootloader_hash: config
                .bootloader_hash
                .context("missing `bootloader_hash`")?,
            default_aa_hash: config
                .default_aa_hash
                .context("missing `default_aa_hash`")?,
            evm_emulator_hash: config.evm_emulator_hash,
            l1_chain_id: config.l1_chain_id,
            l2_chain_id: config.l2_chain_id,
            fee_account: config.fee_account,
            l1_batch_commit_data_generator_mode: config.l1_batch_commit_data_generator_mode,
            custom_genesis_state_path: config.custom_genesis_state_path,
            prover: PersistedGenesisProverConfig {
                dummy_verifier: config.dummy_verifier,
                snark_wrapper_vk_hash: config.snark_wrapper_vk_hash,
                fflonk_snark_wrapper_vk_hash: config.fflonk_snark_wrapper_vk_hash,
            },
        })
    }
}

impl TryFrom<PersistedGenesisConfig> for GenesisConfig {
    type Error = ErrorWithOrigin;

    fn try_from(config: PersistedGenesisConfig) -> Result<Self, Self::Error> {
        Ok(Self {
            protocol_version: Some(match config.genesis_protocol_semantic_version {
                Some(ver) => ver,
                None => {
                    let minor = config.genesis_protocol_version.ok_or_else(|| {
                        DeError::custom("Either genesis_protocol_version or genesis_protocol_semantic_version should be presented")
                    })?;
                    let minor: ProtocolVersionId = minor.try_into().map_err(|_| {
                        DeError::invalid_value(
                            Unexpected::Unsigned(minor.into()),
                            &"protocol version ID",
                        )
                    })?;
                    ProtocolSemanticVersion::new(minor, 0.into())
                }
            }),
            genesis_root_hash: Some(config.genesis_root),
            rollup_last_leaf_index: Some(config.genesis_rollup_leaf_index),
            genesis_commitment: Some(config.genesis_batch_commitment),
            bootloader_hash: Some(config.bootloader_hash),
            default_aa_hash: Some(config.default_aa_hash),
            evm_emulator_hash: config.evm_emulator_hash,
            l1_chain_id: config.l1_chain_id,
            l2_chain_id: config.l2_chain_id,
            snark_wrapper_vk_hash: config.prover.snark_wrapper_vk_hash,
            fee_account: config.fee_account,
            dummy_verifier: config.prover.dummy_verifier,
            l1_batch_commit_data_generator_mode: config.l1_batch_commit_data_generator_mode,
            fflonk_snark_wrapper_vk_hash: config.prover.fflonk_snark_wrapper_vk_hash,
            custom_genesis_state_path: config.custom_genesis_state_path,
        })
    }
}

/// This config represents the genesis state of the chain.
/// Each chain has this config immutable and we update it only during the protocol upgrade
#[derive(Debug, Clone, PartialEq)]
pub struct GenesisConfig {
    // TODO make fields non optional?
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

    /// **Important:** This method uses blocking I/O.
    pub fn read(path: &Path) -> anyhow::Result<Self> {
        let file = fs::File::open(path)
            .with_context(|| format!("failed opening genesis config file at {:?}", path))?;
        let config: PersistedGenesisConfig = serde_yaml::from_reader(BufReader::new(file))
            .context("failed deserializing genesis config")?;
        config.try_into().context("malformed genesis config")
    }

    /// **Important:** This method uses blocking I/O.
    pub fn write(self, path: &Path) -> anyhow::Result<()> {
        let path = path.to_owned();
        let config =
            PersistedGenesisConfig::try_from(self).context("genesis config is incomplete")?;
        let file = fs::File::create(&path)
            .with_context(|| format!("failed creating genesis config file at {:?}", path))?;
        serde_yaml::to_writer(BufWriter::new(file), &config)
            .context("failed serializing config to YAML")
    }
}

#[derive(Debug)]
pub struct GenesisConfigDeserializer;

impl DeserializeParam<GenesisConfig> for GenesisConfigDeserializer {
    const EXPECTING: BasicTypes = BasicTypes::OBJECT;

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<GenesisConfig, ErrorWithOrigin> {
        let de = ctx.current_value_deserializer(param.name)?;
        PersistedGenesisConfig::deserialize(de)?.try_into()
    }

    fn serialize_param(&self, param: &GenesisConfig) -> serde_json::Value {
        let persisted =
            PersistedGenesisConfig::try_from(param.clone()).expect("invalid genesis config");
        serde_json::to_value(persisted).unwrap()
    }
}

impl WellKnown for GenesisConfig {
    type Deserializer = GenesisConfigDeserializer;
    const DE: Self::Deserializer = GenesisConfigDeserializer;
}

impl WellKnownOption for GenesisConfig {}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct GenesisConfigWrapper {
    /// Genesis configuration.
    pub genesis: Option<GenesisConfig>,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Yaml};

    use super::*;

    fn expected_config() -> GenesisConfig {
        GenesisConfig {
            protocol_version: Some("0.25.0".parse().unwrap()),
            genesis_root_hash: Some(
                "0x9b30c35100835c0d811c9d385cc9804816dbceb4461b8fe4cbb8d0d5ecdacdec"
                    .parse()
                    .unwrap(),
            ),
            rollup_last_leaf_index: Some(54),
            genesis_commitment: Some(
                "0x043d432c1b668e54ada198d683516109e45e4f7f81f216ff4c4f469117732e50"
                    .parse()
                    .unwrap(),
            ),
            bootloader_hash: Some(
                "0x010008e15394cd83a8d463d61e00b4361afbc27c932b07a9d2100861b7d05e78"
                    .parse()
                    .unwrap(),
            ),
            default_aa_hash: Some(
                "0x01000523eadd3061f8e701acda503defb7ac3734ae3371e4daf7494651d8b523"
                    .parse()
                    .unwrap(),
            ),
            evm_emulator_hash: None,
            l1_chain_id: L1ChainId(9),
            l2_chain_id: L2ChainId::from(271),
            snark_wrapper_vk_hash:
                "0x14f97b81e54b35fe673d8708cc1a19e1ea5b5e348e12d31e39824ed4f42bbca2"
                    .parse()
                    .unwrap(),
            fee_account: Address::from_low_u64_be(1),
            dummy_verifier: true,
            l1_batch_commit_data_generator_mode: L1BatchCommitmentMode::Rollup,
            fflonk_snark_wrapper_vk_hash: Some(H256::repeat_byte(0xef)),
            custom_genesis_state_path: Some("/db/genesis".to_owned()),
        }
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          genesis:
            genesis_root: 0x9b30c35100835c0d811c9d385cc9804816dbceb4461b8fe4cbb8d0d5ecdacdec
            genesis_rollup_leaf_index: 54
            genesis_batch_commitment: 0x043d432c1b668e54ada198d683516109e45e4f7f81f216ff4c4f469117732e50
            genesis_protocol_version: 25
            default_aa_hash: 0x01000523eadd3061f8e701acda503defb7ac3734ae3371e4daf7494651d8b523
            bootloader_hash: 0x010008e15394cd83a8d463d61e00b4361afbc27c932b07a9d2100861b7d05e78
            l1_chain_id: 9
            l2_chain_id: 271
            fee_account: '0x0000000000000000000000000000000000000001'
            prover:
              dummy_verifier: true
              snark_wrapper_vk_hash: 0x14f97b81e54b35fe673d8708cc1a19e1ea5b5e348e12d31e39824ed4f42bbca2
              fflonk_snark_wrapper_vk_hash: 0xefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefefef
            genesis_protocol_semantic_version: 0.25.0
            l1_batch_commit_data_generator_mode: Rollup
            custom_genesis_state_path: "/db/genesis"
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config: GenesisConfigWrapper = test_complete(yaml).unwrap();
        assert_eq!(config.genesis.unwrap(), expected_config());
    }
}
