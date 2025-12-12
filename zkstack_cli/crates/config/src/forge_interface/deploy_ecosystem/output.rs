use std::collections::HashMap;

use ethers::types::{Address, U256};
use serde::{Deserialize, Serialize};

use crate::{
    consts::ERC20_CONFIGS_FILE,
    forge_interface::Create2Addresses,
    traits::{FileConfigTrait, FileConfigWithDefaultName},
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployL1CoreContractsOutput {
    pub contracts: Create2Addresses,
    pub deployer_addr: Address,
    pub era_chain_id: u32,
    pub l1_chain_id: u32,
    pub owner_address: Address,
    pub deployed_addresses: DeployL1CoreContractsDeployedAddressesOutput,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployL1CoreContractsDeployedAddressesOutput {
    pub governance_addr: Address,
    pub transparent_proxy_admin_addr: Address,
    pub chain_admin: Address,
    pub access_control_restriction_addr: Address,
    pub bridgehub: L1BridgehubOutput,
    pub bridges: L1BridgesOutput,
    pub native_token_vault_addr: Address,
}

impl FileConfigTrait for DeployL1CoreContractsOutput {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployCTMOutput {
    pub contracts_config: DeployCTMContractsConfigOutput,
    pub deployed_addresses: DeployCTMDeployedAddressesOutput,
    pub multicall3_addr: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployCTMDeployedAddressesOutput {
    pub governance_addr: Address,
    pub transparent_proxy_admin_addr: Address,
    pub validator_timelock_addr: Address,
    pub chain_admin: Address,
    pub state_transition: L1StateTransitionOutput,
    pub rollup_l1_da_validator_addr: Address,
    pub no_da_validium_l1_validator_addr: Address,
    pub avail_l1_da_validator_addr: Address,
    pub l1_rollup_da_manager: Address,
    pub server_notifier_proxy_addr: Address,
}

impl FileConfigTrait for DeployCTMOutput {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployCTMContractsConfigOutput {
    pub diamond_cut_data: String,
    pub force_deployments_data: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct L1BridgehubOutput {
    pub bridgehub_implementation_addr: Address,
    pub bridgehub_proxy_addr: Address,
    pub ctm_deployment_tracker_proxy_addr: Address,
    pub ctm_deployment_tracker_implementation_addr: Address,
    pub message_root_proxy_addr: Address,
    pub message_root_implementation_addr: Address,
    pub chain_asset_handler_proxy_addr: Address,
    pub chain_asset_handler_implementation_addr: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct L1BridgesOutput {
    pub erc20_bridge_implementation_addr: Address,
    pub erc20_bridge_proxy_addr: Address,
    pub shared_bridge_implementation_addr: Address,
    pub shared_bridge_proxy_addr: Address,
    pub l1_nullifier_implementation_addr: Address,
    pub l1_nullifier_proxy_addr: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct L1StateTransitionOutput {
    pub state_transition_proxy_addr: Address,
    pub verifier_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub default_upgrade_addr: Address,
    pub bytecodes_supplier_addr: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Erc20Token {
    pub address: Address,
    pub name: String,
    pub symbol: String,
    pub decimals: u64,
    pub implementation: String,
    pub mint: U256,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ERC20Tokens {
    pub tokens: HashMap<String, Erc20Token>,
}

impl FileConfigWithDefaultName for ERC20Tokens {
    const FILE_NAME: &'static str = ERC20_CONFIGS_FILE;
}

impl FileConfigTrait for ERC20Tokens {}
