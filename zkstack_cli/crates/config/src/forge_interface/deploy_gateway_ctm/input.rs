use ethers::abi::Address;
use serde::{Deserialize, Serialize};
use zkstack_cli_types::ProverMode;
use zksync_basic_types::{H256, U256};

use crate::{
    forge_interface::deploy_ecosystem::input::{GenesisInput, InitialDeploymentConfig},
    traits::ZkStackConfig,
    ChainConfig, ContractsConfig, EcosystemConfig,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeployGatewayCTMInput {
    bridgehub_proxy_addr: Address,
    ctm_deployment_tracker_proxy_addr: Address,
    native_token_vault_addr: Address,
    chain_type_manager_proxy_addr: Address,
    shared_bridge_proxy_addr: Address,

    governance: Address,
    base_token: Address,

    chain_chain_id: U256,
    era_chain_id: U256,
    l1_chain_id: U256,

    testnet_verifier: bool,

    recursion_node_level_vk_hash: H256,
    recursion_leaf_level_vk_hash: H256,
    recursion_circuits_set_vks_hash: H256,

    diamond_init_pubdata_pricing_mode: u64,
    diamond_init_batch_overhead_l1_gas: u64,
    diamond_init_max_pubdata_per_batch: u64,
    diamond_init_max_l2_gas_per_batch: u64,
    diamond_init_priority_tx_max_pubdata: u64,
    diamond_init_minimal_l2_gas_price: u64,

    bootloader_hash: H256,
    default_aa_hash: H256,
    evm_emulator_hash: Option<H256>,

    priority_tx_max_gas_limit: u64,

    genesis_root: H256,
    genesis_rollup_leaf_index: u64,
    genesis_batch_commitment: H256,

    latest_protocol_version: U256,

    force_deployments_data: String,

    expected_rollup_l2_da_validator: Address,
}

impl ZkStackConfig for DeployGatewayCTMInput {}

impl DeployGatewayCTMInput {
    pub fn new(
        chain_config: &ChainConfig,
        ecosystem_config: &EcosystemConfig,
        genesis_input: &GenesisInput,
        contracts_config: &ContractsConfig,
        initial_deployment_config: &InitialDeploymentConfig,
    ) -> Self {
        Self {
            bridgehub_proxy_addr: contracts_config.ecosystem_contracts.bridgehub_proxy_addr,
            ctm_deployment_tracker_proxy_addr: contracts_config
                .ecosystem_contracts
                .stm_deployment_tracker_proxy_addr
                .expect("stm_deployment_tracker_proxy_addr"),
            native_token_vault_addr: contracts_config
                .ecosystem_contracts
                .native_token_vault_addr
                .expect("native_token_vault_addr"),
            chain_type_manager_proxy_addr: contracts_config
                .ecosystem_contracts
                .state_transition_proxy_addr,
            shared_bridge_proxy_addr: contracts_config.bridges.shared.l1_address,
            governance: ecosystem_config
                .get_contracts_config()
                .unwrap()
                .l1
                .governance_addr,

            base_token: chain_config.base_token.address,

            chain_chain_id: U256::from(chain_config.chain_id.as_u64()),
            era_chain_id: U256::from(ecosystem_config.era_chain_id.as_u64()),
            l1_chain_id: U256::from(ecosystem_config.l1_network.chain_id()),

            testnet_verifier: ecosystem_config.prover_version == ProverMode::NoProofs,
            recursion_node_level_vk_hash: H256::zero(),
            recursion_leaf_level_vk_hash: H256::zero(),
            recursion_circuits_set_vks_hash: H256::zero(),

            diamond_init_pubdata_pricing_mode: initial_deployment_config
                .diamond_init_pubdata_pricing_mode,
            diamond_init_batch_overhead_l1_gas: initial_deployment_config
                .diamond_init_batch_overhead_l1_gas,
            diamond_init_max_pubdata_per_batch: initial_deployment_config
                .diamond_init_max_pubdata_per_batch,
            diamond_init_max_l2_gas_per_batch: initial_deployment_config
                .diamond_init_max_l2_gas_per_batch,
            diamond_init_priority_tx_max_pubdata: initial_deployment_config
                .diamond_init_priority_tx_max_pubdata,
            diamond_init_minimal_l2_gas_price: initial_deployment_config
                .diamond_init_minimal_l2_gas_price,

            bootloader_hash: genesis_input.bootloader_hash,
            default_aa_hash: genesis_input.default_aa_hash,
            evm_emulator_hash: genesis_input.evm_emulator_hash,

            priority_tx_max_gas_limit: initial_deployment_config.priority_tx_max_gas_limit,

            genesis_root: genesis_input.genesis_root_hash,
            genesis_rollup_leaf_index: genesis_input.rollup_last_leaf_index,
            genesis_batch_commitment: genesis_input.genesis_commitment,

            latest_protocol_version: genesis_input.protocol_version.pack(),

            expected_rollup_l2_da_validator: contracts_config
                .ecosystem_contracts
                .expected_rollup_l2_da_validator
                .unwrap(),

            force_deployments_data: contracts_config
                .ecosystem_contracts
                .force_deployments_data
                .clone()
                .expect("force_deployments_data"),
        }
    }
}
