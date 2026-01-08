use std::{collections::HashMap, str::FromStr};

use ethers::{
    prelude::U256,
    types::{Address, H256},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use zkstack_cli_types::VMOption;
use zksync_basic_types::L2ChainId;

use crate::{
    consts::INITIAL_DEPLOYMENT_FILE,
    traits::{FileConfigTrait, FileConfigWithDefaultName},
    ContractsConfigForDeployERC20, WalletsConfig, ERC20_DEPLOYMENT_FILE,
};

/// Part of the genesis config influencing `DeployGatewayCTMInput`.
#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct InitialDeploymentConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create2_factory_addr: Option<Address>,
    pub create2_factory_salt: H256,
    pub governance_min_delay: u64,
    pub token_weth_address: Address,
    pub bridgehub_create_new_chain_salt: u64,
    pub max_number_of_chains: u64,
    pub validator_timelock_execution_delay: u64,
}

impl Default for InitialDeploymentConfig {
    fn default() -> Self {
        Self {
            create2_factory_addr: None,
            create2_factory_salt: H256::random(),
            governance_min_delay: 0,
            max_number_of_chains: 100,
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
    pub support_l2_legacy_shared_bridge_test: bool,
    pub contracts: ContractsDeployL1Config,
    pub tokens: TokensDeployL1Config,
    pub is_zk_sync_os: bool,
}

impl FileConfigTrait for DeployL1Config {}

impl DeployL1Config {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        wallets_config: &WalletsConfig,
        initial_deployment_config: &InitialDeploymentConfig,
        era_chain_id: L2ChainId,
        support_l2_legacy_shared_bridge_test: bool,
        vm_option: VMOption,
    ) -> Self {
        Self {
            is_zk_sync_os: vm_option.is_zksync_os(),
            era_chain_id,
            owner_address: wallets_config.governor.address,
            support_l2_legacy_shared_bridge_test,
            contracts: ContractsDeployL1Config {
                create2_factory_addr: initial_deployment_config.create2_factory_addr,
                create2_factory_salt: initial_deployment_config.create2_factory_salt,
                // TODO verify correctnesss
                governance_security_council_address: wallets_config.governor.address,
                governance_min_delay: initial_deployment_config.governance_min_delay,
                max_number_of_chains: initial_deployment_config.max_number_of_chains,
                era_diamond_proxy_addr: None,
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
    pub era_diamond_proxy_addr: Option<Address>,
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
