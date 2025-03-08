use ethers::types::{Address, H256};
use serde::{Deserialize, Serialize};
use zksync_basic_types::web3::Bytes;

use crate::traits::{FileConfigWithDefaultName, ZkStackConfig};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct V27EcosystemUpgradeOutput {
    pub create2_factory_addr: Address,
    pub create2_factory_salt: H256,
    pub deployer_addr: Address,
    pub era_chain_id: u32,
    pub l1_chain_id: u32,
    pub owner_address: Address,
    pub chain_upgrade_diamond_cut: Bytes,
    pub protocol_upgrade_handler_proxy_address: Address,
    pub protocol_upgrade_handler_impl_address: Address,
    pub governance_calls: V27GovernanceCalls,

    #[serde(rename = "contracts_newConfig")]
    pub contracts_config: V27EcosystemUpgradeContractsOutput,

    pub deployed_addresses: V27EcosystemUpgradeDeployedAddresses,
    /// List of transactions that were executed during the upgrade.
    /// This is added later by the zkstack and not present in the toml file that solidity creates.
    #[serde(default)]
    pub transactions: Vec<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct V27GovernanceCalls {
    pub governance_stage1_calls: Bytes,
    pub governance_stage2_calls: Bytes,
}

impl FileConfigWithDefaultName for V27EcosystemUpgradeOutput {
    const FILE_NAME: &'static str = "ecosystem_upgrade_output.yaml";
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct V27EcosystemUpgradeContractsOutput {
    pub diamond_cut_data: Bytes,

    pub diamond_init_batch_overhead_l1_gas: u64,
    pub diamond_init_max_l2_gas_per_batch: u64,
    pub diamond_init_max_pubdata_per_batch: u64,
    pub diamond_init_minimal_l2_gas_price: u64,
    pub diamond_init_priority_tx_max_pubdata: u64,
    pub diamond_init_pubdata_pricing_mode: u64,
    pub expected_rollup_l2_da_validator: Address,
    pub expected_validium_l2_da_validator: Address,

    // Probably gonna need it to add new chains
    pub force_deployments_data: Bytes,

    pub priority_tx_max_gas_limit: u64,

    pub recursion_circuits_set_vks_hash: H256,
    pub recursion_leaf_level_vk_hash: H256,
    pub recursion_node_level_vk_hash: H256,

    pub new_protocol_version: u64, // broken
    pub old_protocol_version: u64,

    pub old_validator_timelock: Address,
    pub l1_legacy_shared_bridge: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct V27EcosystemUpgradeDeployedAddresses {
    pub native_token_vault_addr: Address,
    pub native_token_vault_implementation_addr: Address,
    pub rollup_l1_da_validator_addr: Address,
    pub validator_timelock_addr: Address,
    pub validium_l1_da_validator_addr: Address,
    pub l1_bytecodes_supplier_addr: Address,
    pub l2_wrapped_base_token_store_addr: Address,

    pub l1_transitionary_owner: Address,
    pub l1_rollup_da_manager: Address,
    pub l1_governance_upgrade_timer: Address,

    pub bridgehub: V27EcosystemUpgradeBridgehub,
    pub bridges: V27EcosystemUpgradeBridges,
    pub state_transition: V27EcosystemUpgradeStateTransition,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct V27EcosystemUpgradeBridgehub {
    pub bridgehub_implementation_addr: Address,
    pub ctm_deployment_tracker_implementation_addr: Address,
    pub ctm_deployment_tracker_proxy_addr: Address,
    pub message_root_implementation_addr: Address,
    pub message_root_proxy_addr: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct V27EcosystemUpgradeBridges {
    pub erc20_bridge_implementation_addr: Address,
    pub l1_nullifier_implementation_addr: Address,
    pub l1_nullifier_proxy_addr: Address,
    // in the past known as 'shared bridge'
    pub l1_asset_router_implementation_addr: Address,
    pub l1_asset_router_proxy_addr: Address,
    pub bridged_standard_erc20_impl: Address,
    pub bridged_token_beacon: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct V27EcosystemUpgradeStateTransition {
    pub admin_facet_addr: Address,
    pub default_upgrade_addr: Address,
    pub diamond_init_addr: Address,
    pub executor_facet_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub getters_facet_addr: Address,
    pub mailbox_facet_addr: Address,
    pub state_transition_implementation_addr: Address,
    pub verifier_addr: Address,
}

impl ZkStackConfig for V27EcosystemUpgradeOutput {}
