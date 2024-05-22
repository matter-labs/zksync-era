use std::{collections::HashMap, str::FromStr};

use ethers::{
    addressbook::Address,
    core::{rand, rand::Rng},
    prelude::H256,
};
use serde::{Deserialize, Serialize};

use crate::{
    configs::{
        ContractsConfig, GenesisConfig, ReadConfig, SaveConfig, SaveConfigWithComment,
        WalletsConfig,
    },
    types::ChainId,
};

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

impl ReadConfig for InitialDeploymentConfig {}
impl SaveConfig for InitialDeploymentConfig {}
impl SaveConfigWithComment for InitialDeploymentConfig {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Erc20DeploymentConfig {
    pub tokens: Vec<Erc20DeploymentTokensConfig>,
}

impl ReadConfig for Erc20DeploymentConfig {}
impl SaveConfig for Erc20DeploymentConfig {}
impl SaveConfigWithComment for Erc20DeploymentConfig {}

impl Default for Erc20DeploymentConfig {
    fn default() -> Self {
        Self {
            tokens: vec![
                Erc20DeploymentTokensConfig {
                    name: String::from("DAI"),
                    symbol: String::from("DAI"),
                    decimals: 18,
                    implementation: String::from("TestnetERC20Token.sol"),
                    mint: 10000000000,
                },
                Erc20DeploymentTokensConfig {
                    name: String::from("Wrapped Ether"),
                    symbol: String::from("WETH"),
                    decimals: 18,
                    implementation: String::from("WETH9.sol"),
                    mint: 0,
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
    pub mint: u64,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployL1Config {
    pub era_chain_id: ChainId,
    pub owner_address: Address,
    pub testnet_verifier: bool,
    pub contracts: ContractsDeployL1Config,
    pub tokens: TokensDeployL1Config,
}

impl ReadConfig for DeployL1Config {}
impl SaveConfig for DeployL1Config {}

impl DeployL1Config {
    pub fn new(
        genesis_config: &GenesisConfig,
        wallets_config: &WalletsConfig,
        initial_deployment_config: &InitialDeploymentConfig,
        era_chain_id: ChainId,
        testnet_verifier: bool,
    ) -> Self {
        Self {
            era_chain_id,
            testnet_verifier,
            owner_address: wallets_config.governor.address,
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
                bootloader_hash: genesis_config.bootloader_hash,
                default_aa_hash: genesis_config.default_aa_hash,
                diamond_init_priority_tx_max_pubdata: initial_deployment_config
                    .diamond_init_priority_tx_max_pubdata,
                diamond_init_pubdata_pricing_mode: initial_deployment_config
                    .diamond_init_pubdata_pricing_mode,
                genesis_batch_commitment: genesis_config.genesis_batch_commitment,
                genesis_rollup_leaf_index: genesis_config.genesis_rollup_leaf_index,
                genesis_root: genesis_config.genesis_root,
                latest_protocol_version: genesis_config.genesis_protocol_version,
                recursion_circuits_set_vks_hash: H256::zero(),
                recursion_leaf_level_vk_hash: H256::zero(),
                recursion_node_level_vk_hash: H256::zero(),
                priority_tx_max_gas_limit: initial_deployment_config.priority_tx_max_gas_limit,
                validator_timelock_execution_delay: initial_deployment_config
                    .validator_timelock_execution_delay,
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
    pub genesis_rollup_leaf_index: u32,
    pub genesis_batch_commitment: H256,
    pub latest_protocol_version: u64,
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
}

impl ReadConfig for DeployErc20Config {}
impl SaveConfig for DeployErc20Config {}

impl DeployErc20Config {
    pub fn new(
        erc20_deployment_config: &Erc20DeploymentConfig,
        contracts_config: &ContractsConfig,
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
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TokenDeployErc20Config {
    pub name: String,
    pub symbol: String,
    pub decimals: u64,
    pub implementation: String,
    pub mint: u64,
}
