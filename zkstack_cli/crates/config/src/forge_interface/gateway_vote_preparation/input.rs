use ethers::types::{Address, H256, U256};
use serde::{Deserialize, Serialize};

use crate::{
    forge_interface::deploy_ecosystem::input::{GenesisInput, InitialDeploymentConfig},
    traits::ZkStackConfig,
    ContractsConfig,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayContractsConfig {
    pub governance_security_council_address: Address,
    pub governance_min_delay: U256,
    pub max_number_of_chains: U256,
    pub create2_factory_salt: H256,
    pub create2_factory_addr: Option<Address>,
    pub validator_timelock_execution_delay: U256,
    pub genesis_root: H256,
    pub genesis_rollup_leaf_index: U256,
    pub genesis_batch_commitment: H256,
    pub latest_protocol_version: U256,
    pub recursion_node_level_vk_hash: H256,
    pub recursion_leaf_level_vk_hash: H256,
    pub recursion_circuits_set_vks_hash: H256,
    pub priority_tx_max_gas_limit: U256,
    pub diamond_init_pubdata_pricing_mode: U256,
    pub diamond_init_batch_overhead_l1_gas: U256,
    pub diamond_init_max_pubdata_per_batch: U256,
    pub diamond_init_max_l2_gas_per_batch: U256,
    pub diamond_init_priority_tx_max_pubdata: U256,
    pub diamond_init_minimal_l2_gas_price: U256,
    pub default_aa_hash: H256,
    pub bootloader_hash: H256,
    pub evm_emulator_hash: H256,
    pub avail_l1_da_validator: Option<Address>,
    pub bridgehub_proxy_address: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokensConfig {
    pub token_weth_address: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayVotePreparationConfig {
    pub era_chain_id: U256,
    pub owner_address: Address,
    pub testnet_verifier: bool,
    pub support_l2_legacy_shared_bridge_test: bool,
    pub contracts: GatewayContractsConfig,
    pub tokens: TokensConfig,
    pub refund_recipient: Address,
    pub rollup_l2_da_validator: Address,
    pub old_rollup_l2_da_validator: Address,
    pub gateway_chain_id: U256,
    pub force_deployments_data: String,
}

impl ZkStackConfig for GatewayVotePreparationConfig {}

impl GatewayVotePreparationConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        initial_deployment_config: &InitialDeploymentConfig,
        genesis_input: &GenesisInput,
        external_contracts_config: &ContractsConfig, // from external context
        era_chain_id: U256,
        gateway_chain_id: U256,
        owner_address: Address,
        testnet_verifier: bool,
        refund_recipient: Address,
        rollup_l2_da_validator: Address,
        old_rollup_l2_da_validator: Address,
    ) -> Self {
        let contracts = GatewayContractsConfig {
            governance_security_council_address: Address::zero(),
            governance_min_delay: U256::from(initial_deployment_config.governance_min_delay),
            max_number_of_chains: U256::from(initial_deployment_config.max_number_of_chains),
            create2_factory_salt: initial_deployment_config.create2_factory_salt,
            create2_factory_addr: initial_deployment_config.create2_factory_addr,
            validator_timelock_execution_delay: U256::from(
                initial_deployment_config.validator_timelock_execution_delay,
            ),
            genesis_root: genesis_input.genesis_root_hash,
            genesis_rollup_leaf_index: U256::from(genesis_input.rollup_last_leaf_index),
            genesis_batch_commitment: genesis_input.genesis_commitment,
            latest_protocol_version: genesis_input.protocol_version.pack(),
            recursion_node_level_vk_hash: H256::zero(), // These are always zero
            recursion_leaf_level_vk_hash: H256::zero(), // These are always zero
            recursion_circuits_set_vks_hash: H256::zero(), // These are always zero
            priority_tx_max_gas_limit: U256::from(
                initial_deployment_config.priority_tx_max_gas_limit,
            ),
            diamond_init_pubdata_pricing_mode: U256::from(
                initial_deployment_config.diamond_init_pubdata_pricing_mode,
            ),
            diamond_init_batch_overhead_l1_gas: U256::from(
                initial_deployment_config.diamond_init_batch_overhead_l1_gas,
            ),
            diamond_init_max_pubdata_per_batch: U256::from(
                initial_deployment_config.diamond_init_max_pubdata_per_batch,
            ),
            diamond_init_max_l2_gas_per_batch: U256::from(
                initial_deployment_config.diamond_init_max_l2_gas_per_batch,
            ),
            diamond_init_priority_tx_max_pubdata: U256::from(
                initial_deployment_config.diamond_init_priority_tx_max_pubdata,
            ),
            diamond_init_minimal_l2_gas_price: U256::from(
                initial_deployment_config.diamond_init_minimal_l2_gas_price,
            ),
            default_aa_hash: genesis_input.default_aa_hash,
            bootloader_hash: genesis_input.bootloader_hash,
            evm_emulator_hash: genesis_input.evm_emulator_hash.unwrap_or_default(),
            avail_l1_da_validator: external_contracts_config.l1.avail_l1_da_validator_addr,
            bridgehub_proxy_address: external_contracts_config
                .ecosystem_contracts
                .bridgehub_proxy_addr,
        };

        let tokens = TokensConfig {
            token_weth_address: initial_deployment_config.token_weth_address,
        };

        Self {
            era_chain_id,
            owner_address,
            testnet_verifier,
            support_l2_legacy_shared_bridge_test: false,
            contracts,
            tokens,
            refund_recipient,
            rollup_l2_da_validator,
            old_rollup_l2_da_validator,
            gateway_chain_id,
            force_deployments_data: external_contracts_config
                .ecosystem_contracts
                .force_deployments_data
                .clone()
                .unwrap_or_default(),
        }
    }
}
