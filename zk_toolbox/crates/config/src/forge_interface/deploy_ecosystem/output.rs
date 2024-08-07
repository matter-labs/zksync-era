use std::collections::HashMap;

use ethers::types::{Address, H256, U256};
use serde::{Deserialize, Serialize};

use crate::{
    consts::ERC20_CONFIGS_FILE,
    traits::{FileConfigWithDefaultName, ZkToolboxConfig},
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployL1Output {
    pub create2_factory_addr: Address,
    pub create2_factory_salt: H256,
    pub deployer_addr: Address,
    pub era_chain_id: u32,
    pub l1_chain_id: u32,
    pub multicall3_addr: Address,
    pub owner_address: Address,
    pub contracts_config: DeployL1ContractsConfigOutput,
    pub deployed_addresses: DeployL1DeployedAddressesOutput,
}

impl ZkToolboxConfig for DeployL1Output {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployL1ContractsConfigOutput {
    pub diamond_init_max_l2_gas_per_batch: u64,
    pub diamond_init_batch_overhead_l1_gas: u64,
    pub diamond_init_max_pubdata_per_batch: u64,
    pub diamond_init_minimal_l2_gas_price: u64,
    pub diamond_init_priority_tx_max_pubdata: u64,
    pub diamond_init_pubdata_pricing_mode: u64,
    pub priority_tx_max_gas_limit: u64,
    pub recursion_circuits_set_vks_hash: H256,
    pub recursion_leaf_level_vk_hash: H256,
    pub recursion_node_level_vk_hash: H256,
    pub diamond_cut_data: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployL1DeployedAddressesOutput {
    pub blob_versioned_hash_retriever_addr: Address,
    pub governance_addr: Address,
    pub transparent_proxy_admin_addr: Address,
    pub validator_timelock_addr: Address,
    pub bridgehub: L1BridgehubOutput,
    pub bridges: L1BridgesOutput,
    pub state_transition: L1StateTransitionOutput,
    pub rollup_l1_da_validator_addr: Address,
    pub validium_l1_da_validator_addr: Address
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct L1BridgehubOutput {
    pub bridgehub_implementation_addr: Address,
    pub bridgehub_proxy_addr: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct L1BridgesOutput {
    pub erc20_bridge_implementation_addr: Address,
    pub erc20_bridge_proxy_addr: Address,
    pub shared_bridge_implementation_addr: Address,
    pub shared_bridge_proxy_addr: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct L1StateTransitionOutput {
    pub admin_facet_addr: Address,
    pub default_upgrade_addr: Address,
    pub diamond_init_addr: Address,
    pub diamond_proxy_addr: Address,
    pub executor_facet_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub getters_facet_addr: Address,
    pub mailbox_facet_addr: Address,
    pub state_transition_implementation_addr: Address,
    pub state_transition_proxy_addr: Address,
    pub verifier_addr: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct TokenDeployErc20Output {
    pub address: Address,
    pub name: String,
    pub symbol: String,
    pub decimals: u64,
    pub implementation: String,
    pub mint: U256,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployErc20Output {
    pub tokens: HashMap<String, TokenDeployErc20Output>,
}

impl FileConfigWithDefaultName for DeployErc20Output {
    const FILE_NAME: &'static str = ERC20_CONFIGS_FILE;
}

impl ZkToolboxConfig for DeployErc20Output {}
