#![allow(dead_code)]
//! Copied from `zkstack_cli_config::contracts`

use serde::Deserialize;
use zksync_types::{Address, H256};

#[derive(Debug, Deserialize, Clone, Default)]
pub struct ContractsConfig {
    pub create2_factory_addr: Address,
    pub create2_factory_salt: H256,
    pub ecosystem_contracts: EcosystemContracts,
    pub bridges: BridgesContracts,
    pub l1: L1Contracts,
    pub l2: L2Contracts,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
pub struct EcosystemContracts {
    pub bridgehub_proxy_addr: Address,
    pub state_transition_proxy_addr: Address,
    pub transparent_proxy_admin_addr: Address,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stm_deployment_tracker_proxy_addr: Option<Address>,
    pub validator_timelock_addr: Address,
    pub diamond_cut_data: String,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub force_deployments_data: Option<String>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_token_vault_addr: Option<Address>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l1_bytecodes_supplier_addr: Option<Address>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_rollup_l2_da_validator: Option<Address>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l1_wrapped_base_token_store: Option<Address>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct BridgesContracts {
    pub erc20: BridgeContractsDefinition,
    pub shared: BridgeContractsDefinition,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l1_nullifier_addr: Option<Address>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct BridgeContractsDefinition {
    pub l1_address: Address,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l2_address: Option<Address>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct L1Contracts {
    pub default_upgrade_addr: Address,
    pub diamond_proxy_addr: Address,
    pub governance_addr: Address,
    #[serde(default)]
    pub chain_admin_addr: Address,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control_restriction_addr: Option<Address>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_proxy_admin_addr: Option<Address>,
    pub multicall3_addr: Address,
    pub verifier_addr: Address,
    pub validator_timelock_addr: Address,
    pub base_token_addr: Address,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_token_asset_id: Option<H256>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollup_l1_da_validator_addr: Option<Address>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avail_l1_da_validator_addr: Option<Address>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub no_da_validium_l1_validator_addr: Option<Address>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transaction_filterer_addr: Option<Address>,
}

#[derive(Debug, Deserialize, Clone, Default)]
pub struct L2Contracts {
    pub testnet_paymaster_addr: Address,
    pub default_l2_upgrader: Address,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub da_validator_addr: Option<Address>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l2_native_token_vault_proxy_addr: Option<Address>,
    // `Option` to be able to parse configs from previous protocol version
    #[serde(skip_serializing_if = "Option::is_none")]
    pub legacy_shared_bridge_addr: Option<Address>,
    pub consensus_registry: Option<Address>,
    pub multicall3: Option<Address>,
    pub timestamp_asserter_addr: Option<Address>,
}
