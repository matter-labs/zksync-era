use ethers::types::{Address, H256};
use serde::{Deserialize, Serialize};
use zksync_basic_types::L2ChainId;

use crate::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig, traits::ZkStackConfig,
    ContractsConfig, GenesisConfig,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GatewayEcosystemUpgradeInput {
    pub era_chain_id: L2ChainId,
    pub owner_address: Address,
    pub testnet_verifier: bool,
    pub contracts: GatewayUpgradeContractsConfig,
    pub tokens: GatewayUpgradeTokensConfig,
    pub governance_upgrade_timer_initial_delay: u64,
}

impl ZkStackConfig for GatewayEcosystemUpgradeInput {}

impl GatewayEcosystemUpgradeInput {
    pub fn new(
        new_genesis_config: &GenesisConfig,
        current_contracts_config: &ContractsConfig,
        // It is expected to not change between the versions
        initial_deployment_config: &InitialDeploymentConfig,
        era_chain_id: L2ChainId,
        era_diamond_proxy: Address,
        testnet_verifier: bool,
    ) -> Self {
        Self {
            era_chain_id,
            testnet_verifier,
            owner_address: current_contracts_config.l1.governance_addr,
            // TODO: for local testing, even 0 is fine - but before prod, we should load it from some configuration.
            governance_upgrade_timer_initial_delay: 0,
            contracts: GatewayUpgradeContractsConfig {
                create2_factory_addr: initial_deployment_config.create2_factory_addr,
                create2_factory_salt: initial_deployment_config.create2_factory_salt,
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
                bootloader_hash: new_genesis_config.bootloader_hash.unwrap(),
                default_aa_hash: new_genesis_config.default_aa_hash.unwrap(),
                diamond_init_priority_tx_max_pubdata: initial_deployment_config
                    .diamond_init_priority_tx_max_pubdata,
                diamond_init_pubdata_pricing_mode: initial_deployment_config
                    .diamond_init_pubdata_pricing_mode,
                // These values are not optional in genesis config with file based configuration
                genesis_batch_commitment: new_genesis_config.genesis_commitment.unwrap(),
                genesis_rollup_leaf_index: new_genesis_config.rollup_last_leaf_index.unwrap(),
                genesis_root: new_genesis_config.genesis_root_hash.unwrap(),
                recursion_circuits_set_vks_hash: H256::zero(),
                recursion_leaf_level_vk_hash: H256::zero(),
                recursion_node_level_vk_hash: H256::zero(),
                priority_tx_max_gas_limit: initial_deployment_config.priority_tx_max_gas_limit,
                validator_timelock_execution_delay: initial_deployment_config
                    .validator_timelock_execution_delay,

                bridgehub_proxy_address: current_contracts_config
                    .ecosystem_contracts
                    .bridgehub_proxy_addr,
                old_shared_bridge_proxy_address: current_contracts_config.bridges.shared.l1_address,
                state_transition_manager_address: current_contracts_config
                    .ecosystem_contracts
                    .state_transition_proxy_addr,
                transparent_proxy_admin: current_contracts_config
                    .ecosystem_contracts
                    .transparent_proxy_admin_addr,
                era_diamond_proxy,
                legacy_erc20_bridge_address: current_contracts_config.bridges.erc20.l1_address,
                old_validator_timelock: current_contracts_config
                    .ecosystem_contracts
                    .validator_timelock_addr,
            },
            tokens: GatewayUpgradeTokensConfig {
                token_weth_address: initial_deployment_config.token_weth_address,
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GatewayUpgradeContractsConfig {
    pub governance_min_delay: u64,
    pub max_number_of_chains: u64,
    pub create2_factory_salt: H256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create2_factory_addr: Option<Address>,
    pub validator_timelock_execution_delay: u64,
    pub genesis_root: H256,
    pub genesis_rollup_leaf_index: u64,
    pub genesis_batch_commitment: H256,
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

    pub bridgehub_proxy_address: Address,
    pub old_shared_bridge_proxy_address: Address,
    pub state_transition_manager_address: Address,
    pub transparent_proxy_admin: Address,
    pub era_diamond_proxy: Address,
    pub legacy_erc20_bridge_address: Address,
    pub old_validator_timelock: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GatewayUpgradeTokensConfig {
    pub token_weth_address: Address,
}
