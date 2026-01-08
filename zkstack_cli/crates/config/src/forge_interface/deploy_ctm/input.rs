use ethers::{
    prelude::U256,
    types::{Address, H256},
};
use serde::{Deserialize, Serialize};
use zkstack_cli_types::{L1Network, VMOption};
use zksync_basic_types::{protocol_version::ProtocolSemanticVersion, u256_to_h256};

use crate::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig, traits::FileConfigTrait,
    ContractsGenesisConfig, WalletsConfig,
};

/// Part of the genesis config influencing `DeployGatewayCTMInput`.
#[derive(Debug)]
pub struct GenesisInput {
    pub bootloader_hash: Option<H256>,
    pub default_aa_hash: Option<H256>,
    pub evm_emulator_hash: Option<H256>,
    pub rollup_last_leaf_index: Option<u64>,
    pub genesis_commitment: H256,
    pub genesis_root_hash: H256,
    pub protocol_version: ProtocolSemanticVersion,
}

impl GenesisInput {
    pub fn new(
        contract_genesis_config: &ContractsGenesisConfig,
        vmoption: VMOption,
    ) -> anyhow::Result<Self> {
        let protocol_version = contract_genesis_config.protocol_semantic_version()?;
        match vmoption {
            VMOption::EraVM => Ok(Self {
                bootloader_hash: Some(contract_genesis_config.0.get("bootloader_hash")?),
                default_aa_hash: Some(contract_genesis_config.0.get("default_aa_hash")?),
                evm_emulator_hash: Some(
                    contract_genesis_config
                        .0
                        .get_opt("evm_emulator_hash")?
                        .unwrap_or_default(),
                ),
                genesis_root_hash: contract_genesis_config.0.get("genesis_root")?,
                rollup_last_leaf_index: Some(
                    contract_genesis_config.0.get("genesis_rollup_leaf_index")?,
                ),
                genesis_commitment: contract_genesis_config.0.get("genesis_batch_commitment")?,
                protocol_version,
            }),
            VMOption::ZKSyncOsVM => {
                let one = u256_to_h256(U256::one());
                let genesis_root = contract_genesis_config.0.get("genesis_root")?;
                Ok(Self {
                    genesis_root_hash: genesis_root,
                    // Placeholders, not used in zkSync OS mode. But necessary to be provided.
                    genesis_commitment: one,
                    bootloader_hash: None,
                    default_aa_hash: None,
                    evm_emulator_hash: None,
                    rollup_last_leaf_index: None,
                    protocol_version,
                })
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployCTMConfig {
    pub owner_address: Address,
    pub testnet_verifier: bool,
    pub support_l2_legacy_shared_bridge_test: bool,
    pub contracts: ContractsDeployCTMConfig,
    pub is_zk_sync_os: bool,
}

impl FileConfigTrait for DeployCTMConfig {}

impl DeployCTMConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        genesis_input: &GenesisInput,
        wallets_config: &WalletsConfig,
        initial_deployment_config: &InitialDeploymentConfig,
        testnet_verifier: bool,
        l1_network: L1Network,
        support_l2_legacy_shared_bridge_test: bool,
        vm_option: VMOption,
    ) -> Self {
        Self {
            is_zk_sync_os: vm_option.is_zksync_os(),
            testnet_verifier,
            owner_address: wallets_config.governor.address,
            support_l2_legacy_shared_bridge_test,
            contracts: ContractsDeployCTMConfig {
                create2_factory_addr: initial_deployment_config.create2_factory_addr,
                create2_factory_salt: initial_deployment_config.create2_factory_salt,
                // TODO verify correctnesss
                governance_security_council_address: wallets_config.governor.address,
                governance_min_delay: initial_deployment_config.governance_min_delay,
                bootloader_hash: genesis_input.bootloader_hash,
                default_aa_hash: genesis_input.default_aa_hash,
                evm_emulator_hash: genesis_input.evm_emulator_hash,
                // These values are not optional in genesis config with file based configuration
                genesis_batch_commitment: genesis_input.genesis_commitment,
                genesis_rollup_leaf_index: genesis_input.rollup_last_leaf_index,
                genesis_root: genesis_input.genesis_root_hash,
                latest_protocol_version: genesis_input.protocol_version.pack(),
                validator_timelock_execution_delay: initial_deployment_config
                    .validator_timelock_execution_delay,
                avail_l1_da_validator_addr: l1_network.avail_l1_da_validator_addr(),
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContractsDeployCTMConfig {
    pub create2_factory_salt: H256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create2_factory_addr: Option<Address>,
    pub governance_security_council_address: Address,
    pub governance_min_delay: u64,
    pub validator_timelock_execution_delay: u64,
    pub genesis_root: H256,
    pub genesis_batch_commitment: H256,
    pub latest_protocol_version: U256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub genesis_rollup_leaf_index: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bootloader_hash: Option<H256>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_aa_hash: Option<H256>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evm_emulator_hash: Option<H256>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avail_l1_da_validator_addr: Option<Address>,
}
