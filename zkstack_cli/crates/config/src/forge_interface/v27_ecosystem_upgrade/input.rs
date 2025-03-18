use ethers::types::{Address, H256};
use serde::{Deserialize, Serialize};
use zksync_basic_types::L2ChainId;

use crate::{
    forge_interface::deploy_ecosystem::input::{GenesisInput, InitialDeploymentConfig},
    traits::ZkStackConfig,
    ContractsConfig,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct V27EcosystemUpgradeInput {
    pub era_chain_id: L2ChainId,
    pub owner_address: Address,
    pub testnet_verifier: bool,
    pub support_l2_legacy_shared_bridge_test: bool,
    pub old_protocol_version: u64,

    pub contracts: V27UpgradeContractsConfig,
    pub tokens: V27UpgradeTokensConfig,
    pub governance_upgrade_timer_initial_delay: u64,
}

impl ZkStackConfig for V27EcosystemUpgradeInput {}

const PREVIOUS_PROTOCOL_VERSION: u64 = 26 << 32;
const LATEST_PROTOCOL_VERSION: u64 = 27 << 32;

impl V27EcosystemUpgradeInput {
    pub fn new(
        new_genesis_input: &GenesisInput,
        current_contracts_config: &ContractsConfig,
        // It is expected to not change between the versions
        initial_deployment_config: &InitialDeploymentConfig,
        era_chain_id: L2ChainId,
        testnet_verifier: bool,
    ) -> Self {
        Self {
            era_chain_id,
            testnet_verifier,
            owner_address: current_contracts_config.l1.governance_addr,
            // FIXME
            support_l2_legacy_shared_bridge_test: false,
            old_protocol_version: PREVIOUS_PROTOCOL_VERSION,
            // TODO: for local testing, even 0 is fine - but before prod, we should load it from some configuration.
            governance_upgrade_timer_initial_delay: 0,
            contracts: V27UpgradeContractsConfig {
                create2_factory_addr: current_contracts_config.create2_factory_addr,
                create2_factory_salt: current_contracts_config.create2_factory_salt,
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
                bootloader_hash: new_genesis_input.bootloader_hash,
                default_aa_hash: new_genesis_input.default_aa_hash,
                evm_emulator_hash: new_genesis_input
                    .evm_emulator_hash
                    .expect("EVM emulator hash is required"),
                diamond_init_priority_tx_max_pubdata: initial_deployment_config
                    .diamond_init_priority_tx_max_pubdata,
                diamond_init_pubdata_pricing_mode: initial_deployment_config
                    .diamond_init_pubdata_pricing_mode,
                // These values are not optional in genesis config with file based configuration
                genesis_batch_commitment: new_genesis_input.genesis_commitment,
                genesis_rollup_leaf_index: new_genesis_input.rollup_last_leaf_index,
                genesis_root: new_genesis_input.genesis_root_hash,
                recursion_circuits_set_vks_hash: H256::zero(),
                recursion_leaf_level_vk_hash: H256::zero(),
                recursion_node_level_vk_hash: H256::zero(),
                priority_tx_max_gas_limit: initial_deployment_config.priority_tx_max_gas_limit,
                validator_timelock_execution_delay: initial_deployment_config
                    .validator_timelock_execution_delay,

                bridgehub_proxy_address: current_contracts_config
                    .ecosystem_contracts
                    .bridgehub_proxy_addr,
                transparent_proxy_admin: current_contracts_config
                    .ecosystem_contracts
                    .transparent_proxy_admin_addr,
                l1_bytecodes_supplier_addr: current_contracts_config
                    .ecosystem_contracts
                    .l1_bytecodes_supplier_addr
                    .unwrap(),
                // For local setup - the governance addr is the 'upgrade handler / owner'
                protocol_upgrade_handler_proxy_address: current_contracts_config.l1.governance_addr,
                // FIXME
                governance_security_council_address: Address::zero(),

                protocol_upgrade_handler_impl_address: Address::zero(),

                latest_protocol_version: LATEST_PROTOCOL_VERSION,
            },
            tokens: V27UpgradeTokensConfig {
                token_weth_address: initial_deployment_config.token_weth_address,
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct V27UpgradeContractsConfig {
    pub governance_min_delay: u64,
    pub max_number_of_chains: u64,
    pub create2_factory_salt: H256,
    pub create2_factory_addr: Address,
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
    pub evm_emulator_hash: H256,

    pub bridgehub_proxy_address: Address,
    pub transparent_proxy_admin: Address,

    pub governance_security_council_address: Address,
    pub latest_protocol_version: u64,
    pub l1_bytecodes_supplier_addr: Address,

    pub protocol_upgrade_handler_proxy_address: Address,
    pub protocol_upgrade_handler_impl_address: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct V27UpgradeTokensConfig {
    pub token_weth_address: Address,
}
