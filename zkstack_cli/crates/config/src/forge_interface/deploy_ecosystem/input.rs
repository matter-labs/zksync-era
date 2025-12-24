use std::{collections::HashMap, str::FromStr};

use ethers::{
    prelude::U256,
    types::{Address, H256},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use zkstack_cli_types::{L1Network, VMOption};
use zksync_basic_types::{
    protocol_version::{ProtocolSemanticVersion, ProtocolVersionId},
    u256_to_h256, L2ChainId,
};

use crate::{
    consts::INITIAL_DEPLOYMENT_FILE,
    traits::{FileConfigTrait, FileConfigWithDefaultName},
    ContractsConfigForDeployERC20, GenesisConfig, WalletsConfig, ERC20_DEPLOYMENT_FILE,
};

/// Part of the genesis config influencing `DeployGatewayCTMInput`.
#[derive(Debug)]
pub struct GenesisInput {
    pub bootloader_hash: H256,
    pub default_aa_hash: H256,
    pub evm_emulator_hash: H256,
    pub genesis_root_hash: H256,
    pub rollup_last_leaf_index: u64,
    pub genesis_commitment: H256,
    pub protocol_version: ProtocolSemanticVersion,
}

impl GenesisInput {
    // FIXME: is this enough? (cf. aliases in the "real" config definition)
    // For supporting zksync os genesis file, we need to have only genesis_root.
    pub fn new(raw: &GenesisConfig, vmoption: VMOption) -> anyhow::Result<Self> {
        match vmoption {
            VMOption::EraVM => Ok(Self {
                bootloader_hash: raw.0.get("bootloader_hash")?,
                default_aa_hash: raw.0.get("default_aa_hash")?,
                evm_emulator_hash: raw.0.get_opt("evm_emulator_hash")?.unwrap_or_default(),
                genesis_root_hash: raw.0.get("genesis_root")?,
                rollup_last_leaf_index: raw.0.get("genesis_rollup_leaf_index")?,
                genesis_commitment: raw.0.get("genesis_batch_commitment")?,
                protocol_version: raw.0.get("genesis_protocol_semantic_version")?,
            }),
            VMOption::ZKSyncOsVM => {
                let one = u256_to_h256(U256::one());
                let genesis_root = raw.0.get("genesis_root")?;
                Ok(Self {
                    genesis_root_hash: genesis_root,
                    // Placeholders, not used in zkSync OS mode. But necessary to be provided.
                    genesis_commitment: one,
                    bootloader_hash: one,
                    default_aa_hash: one,
                    evm_emulator_hash: one,
                    rollup_last_leaf_index: 0,
                    protocol_version: ProtocolSemanticVersion::new(
                        ProtocolVersionId::Version30,
                        0.into(),
                    ),
                })
            }
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InitialDeploymentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create2_factory_addr: Option<Address>,
    pub create2_factory_salt: H256,
    pub governance_min_delay: u64,
    pub max_number_of_chains: u64,
    pub diamond_init_batch_overhead_l1_gas: u64,
    pub diamond_init_max_l2_gas_per_batch: u64,
    pub diamond_init_max_pubdata_per_batch: u64,
    pub diamond_init_minimal_l2_gas_price: u64,
    pub diamond_init_priority_tx_max_pubdata: u64,
    pub diamond_init_pubdata_pricing_mode: u64,
    pub priority_tx_max_gas_limit: u64,
    pub validator_timelock_execution_delay: u64,
    pub token_weth_address: Address,
    pub bridgehub_create_new_chain_salt: u64,
}

impl Default for InitialDeploymentConfig {
    fn default() -> Self {
        Self {
            create2_factory_addr: None,
            create2_factory_salt: H256::random(),
            governance_min_delay: 0,
            max_number_of_chains: 100,
            diamond_init_batch_overhead_l1_gas: 1000000,
            diamond_init_max_l2_gas_per_batch: 80000000,
            diamond_init_max_pubdata_per_batch: 120000,
            diamond_init_minimal_l2_gas_price: 250000000,
            diamond_init_priority_tx_max_pubdata: 99000,
            diamond_init_pubdata_pricing_mode: 0,
            priority_tx_max_gas_limit: 72000000,
            validator_timelock_execution_delay: 0,
            token_weth_address: Address::from_str("0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2")
                .unwrap(),
            // toml crate u64 support is backed by i64 implementation
            // https://github.com/toml-rs/toml/issues/705
            bridgehub_create_new_chain_salt: rand::thread_rng().gen_range(0..=i64::MAX) as u64,
        }
    }
}

impl FileConfigWithDefaultName for InitialDeploymentConfig {
    const FILE_NAME: &'static str = INITIAL_DEPLOYMENT_FILE;
}

impl FileConfigTrait for InitialDeploymentConfig {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Erc20DeploymentConfig {
    pub tokens: Vec<Erc20DeploymentTokensConfig>,
}

impl FileConfigWithDefaultName for Erc20DeploymentConfig {
    const FILE_NAME: &'static str = ERC20_DEPLOYMENT_FILE;
}

impl FileConfigTrait for Erc20DeploymentConfig {}

impl Default for Erc20DeploymentConfig {
    fn default() -> Self {
        Self {
            tokens: vec![
                Erc20DeploymentTokensConfig {
                    name: String::from("DAI"),
                    symbol: String::from("DAI"),
                    decimals: 18,
                    implementation: String::from("TestnetERC20Token.sol"),
                    mint: U256::from_str("9000000000000000000000").unwrap(),
                },
                Erc20DeploymentTokensConfig {
                    name: String::from("WBTC"),
                    symbol: String::from("WBTC"),
                    decimals: 8,
                    implementation: String::from("TestnetERC20Token.sol"),
                    mint: U256::from_str("9000000000000000000000").unwrap(),
                },
            ],
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Erc20DeploymentTokensConfig {
    pub name: String,
    pub symbol: String,
    pub decimals: u64,
    pub implementation: String,
    pub mint: U256,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployL1Config {
    pub era_chain_id: L2ChainId,
    pub owner_address: Address,
    pub testnet_verifier: bool,
    pub support_l2_legacy_shared_bridge_test: bool,
    pub contracts: ContractsDeployL1Config,
    pub tokens: TokensDeployL1Config,
    pub is_zk_sync_os: bool,
}

impl FileConfigTrait for DeployL1Config {}

impl DeployL1Config {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        genesis_input: &GenesisInput,
        wallets_config: &WalletsConfig,
        initial_deployment_config: &InitialDeploymentConfig,
        era_chain_id: L2ChainId,
        testnet_verifier: bool,
        l1_network: L1Network,
        support_l2_legacy_shared_bridge_test: bool,
        vm_option: VMOption,
    ) -> Self {
        Self {
            is_zk_sync_os: vm_option.is_zksync_os(),
            era_chain_id,
            testnet_verifier,
            owner_address: wallets_config.governor.address,
            support_l2_legacy_shared_bridge_test,
            contracts: ContractsDeployL1Config {
                create2_factory_addr: initial_deployment_config.create2_factory_addr,
                create2_factory_salt: initial_deployment_config.create2_factory_salt,
                // TODO verify correctnesss
                governance_security_council_address: wallets_config.governor.address,
                governance_min_delay: initial_deployment_config.governance_min_delay,
                max_number_of_chains: initial_deployment_config.max_number_of_chains,
                diamond_init_batch_overhead_l1_gas: initial_deployment_config
                    .diamond_init_batch_overhead_l1_gas,
                diamond_init_max_l2_gas_per_batch: initial_deployment_config
                    .diamond_init_max_l2_gas_per_batch,
                diamond_init_max_pubdata_per_batch: initial_deployment_config
                    .diamond_init_max_pubdata_per_batch,
                diamond_init_minimal_l2_gas_price: initial_deployment_config
                    .diamond_init_minimal_l2_gas_price,
                bootloader_hash: genesis_input.bootloader_hash,
                default_aa_hash: genesis_input.default_aa_hash,
                evm_emulator_hash: genesis_input.evm_emulator_hash,
                diamond_init_priority_tx_max_pubdata: initial_deployment_config
                    .diamond_init_priority_tx_max_pubdata,
                diamond_init_pubdata_pricing_mode: initial_deployment_config
                    .diamond_init_pubdata_pricing_mode,
                // These values are not optional in genesis config with file based configuration
                genesis_batch_commitment: genesis_input.genesis_commitment,
                genesis_rollup_leaf_index: genesis_input.rollup_last_leaf_index,
                genesis_root: genesis_input.genesis_root_hash,
                latest_protocol_version: genesis_input.protocol_version.pack(),
                recursion_circuits_set_vks_hash: H256::zero(),
                recursion_leaf_level_vk_hash: H256::zero(),
                recursion_node_level_vk_hash: H256::zero(),
                priority_tx_max_gas_limit: initial_deployment_config.priority_tx_max_gas_limit,
                validator_timelock_execution_delay: initial_deployment_config
                    .validator_timelock_execution_delay,
                avail_l1_da_validator_addr: l1_network.avail_l1_da_validator_addr(),
            },
            tokens: TokensDeployL1Config {
                token_weth_address: initial_deployment_config.token_weth_address,
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContractsDeployL1Config {
    pub governance_security_council_address: Address,
    pub governance_min_delay: u64,
    pub max_number_of_chains: u64,
    pub create2_factory_salt: H256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create2_factory_addr: Option<Address>,
    pub validator_timelock_execution_delay: u64,
    pub genesis_root: H256,
    pub genesis_rollup_leaf_index: u64,
    pub genesis_batch_commitment: H256,
    pub latest_protocol_version: U256,
    pub recursion_node_level_vk_hash: H256,
    pub recursion_leaf_level_vk_hash: H256,
    pub recursion_circuits_set_vks_hash: H256,
    pub priority_tx_max_gas_limit: u64,
    pub diamond_init_pubdata_pricing_mode: u64,
    pub diamond_init_batch_overhead_l1_gas: u64,
    pub diamond_init_max_pubdata_per_batch: u64,
    pub diamond_init_max_l2_gas_per_batch: u64,
    pub diamond_init_priority_tx_max_pubdata: u64,
    pub diamond_init_minimal_l2_gas_price: u64,
    pub bootloader_hash: H256,
    pub default_aa_hash: H256,
    pub evm_emulator_hash: H256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avail_l1_da_validator_addr: Option<Address>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TokensDeployL1Config {
    pub token_weth_address: Address,
}

// TODO check for ability to resuse Erc20DeploymentConfig
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployErc20Config {
    pub create2_factory_salt: H256,
    pub create2_factory_addr: Address,
    pub tokens: HashMap<String, TokenDeployErc20Config>,
    pub additional_addresses_for_minting: Vec<Address>,
}

impl FileConfigTrait for DeployErc20Config {}

impl DeployErc20Config {
    pub fn new(
        erc20_deployment_config: &Erc20DeploymentConfig,
        contracts_config: &ContractsConfigForDeployERC20,
        additional_addresses_for_minting: Vec<Address>,
    ) -> Self {
        let mut tokens = HashMap::new();
        for token in &erc20_deployment_config.tokens {
            tokens.insert(
                token.symbol.clone(),
                TokenDeployErc20Config {
                    name: token.name.clone(),
                    symbol: token.symbol.clone(),
                    decimals: token.decimals,
                    implementation: token.implementation.clone(),
                    mint: token.mint,
                },
            );
        }
        Self {
            create2_factory_addr: contracts_config.create2_factory_addr,
            create2_factory_salt: contracts_config.create2_factory_salt,
            tokens,
            additional_addresses_for_minting,
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TokenDeployErc20Config {
    pub name: String,
    pub symbol: String,
    pub decimals: u64,
    pub implementation: String,
    pub mint: U256,
}
