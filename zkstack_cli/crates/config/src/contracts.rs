use std::str::FromStr;

use ethers::types::{Address, H256};
use serde::{Deserialize, Serialize};
use zksync_basic_types::H160;
use zksync_system_constants::{L2_ASSET_ROUTER_ADDRESS, L2_NATIVE_TOKEN_VAULT_ADDRESS};

use crate::{
    consts::CONTRACTS_FILE,
    forge_interface::{
        deploy_ecosystem::output::DeployL1Output,
        deploy_l2_contracts::output::{
            ConsensusRegistryOutput, DefaultL2UpgradeOutput, InitializeBridgeOutput,
            L2DAValidatorAddressOutput, Multicall3Output, TimestampAsserterOutput,
        },
        register_chain::output::RegisterChainOutput,
    },
    traits::{FileConfigWithDefaultName, ZkStackConfig},
};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ContractsConfig {
    pub create2_factory_addr: Address,
    pub create2_factory_salt: H256,
    pub ecosystem_contracts: EcosystemContracts,
    pub bridges: BridgesContracts,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof_manager_contracts: Option<EthProofManagerContracts>,
    pub l1: L1Contracts,
    pub l2: L2Contracts,
    #[serde(flatten)]
    pub other: serde_json::Value,
}

impl ContractsConfig {
    pub fn update_from_l1_output(&mut self, deploy_l1_output: &DeployL1Output) {
        self.create2_factory_addr = deploy_l1_output.create2_factory_addr;
        self.create2_factory_salt = deploy_l1_output.create2_factory_salt;
        self.bridges.erc20.l1_address = deploy_l1_output
            .deployed_addresses
            .bridges
            .erc20_bridge_proxy_addr;
        self.bridges.shared.l1_address = deploy_l1_output
            .deployed_addresses
            .bridges
            .shared_bridge_proxy_addr;
        self.bridges.l1_nullifier_addr = Some(
            deploy_l1_output
                .deployed_addresses
                .bridges
                .l1_nullifier_proxy_addr,
        );
        self.ecosystem_contracts.bridgehub_proxy_addr = deploy_l1_output
            .deployed_addresses
            .bridgehub
            .bridgehub_proxy_addr;
        self.ecosystem_contracts.state_transition_proxy_addr = deploy_l1_output
            .deployed_addresses
            .state_transition
            .state_transition_proxy_addr;
        self.ecosystem_contracts.transparent_proxy_admin_addr = deploy_l1_output
            .deployed_addresses
            .transparent_proxy_admin_addr;
        self.ecosystem_contracts.l1_bytecodes_supplier_addr = Some(
            deploy_l1_output
                .deployed_addresses
                .state_transition
                .bytecodes_supplier_addr,
        );
        self.ecosystem_contracts.stm_deployment_tracker_proxy_addr = Some(
            deploy_l1_output
                .deployed_addresses
                .bridgehub
                .ctm_deployment_tracker_proxy_addr,
        );
        self.ecosystem_contracts.force_deployments_data = Some(
            deploy_l1_output
                .contracts_config
                .force_deployments_data
                .clone(),
        );
        self.ecosystem_contracts.expected_rollup_l2_da_validator =
            Some(deploy_l1_output.expected_rollup_l2_da_validator_addr);
        self.l1.default_upgrade_addr = deploy_l1_output
            .deployed_addresses
            .state_transition
            .default_upgrade_addr;
        self.l1.diamond_proxy_addr = deploy_l1_output
            .deployed_addresses
            .state_transition
            .diamond_proxy_addr;
        self.l1.governance_addr = deploy_l1_output.deployed_addresses.governance_addr;
        self.l1.multicall3_addr = deploy_l1_output.multicall3_addr;
        self.ecosystem_contracts.validator_timelock_addr =
            deploy_l1_output.deployed_addresses.validator_timelock_addr;
        self.ecosystem_contracts.native_token_vault_addr =
            Some(deploy_l1_output.deployed_addresses.native_token_vault_addr);
        self.l1.verifier_addr = deploy_l1_output
            .deployed_addresses
            .state_transition
            .verifier_addr;
        self.l1.validator_timelock_addr =
            deploy_l1_output.deployed_addresses.validator_timelock_addr;
        self.ecosystem_contracts
            .diamond_cut_data
            .clone_from(&deploy_l1_output.contracts_config.diamond_cut_data);
        self.l1.rollup_l1_da_validator_addr = Some(
            deploy_l1_output
                .deployed_addresses
                .rollup_l1_da_validator_addr,
        );
        self.l1.no_da_validium_l1_validator_addr = Some(
            deploy_l1_output
                .deployed_addresses
                .no_da_validium_l1_validator_addr,
        );
        self.l1.avail_l1_da_validator_addr = Some(
            deploy_l1_output
                .deployed_addresses
                .avail_l1_da_validator_addr,
        );
        self.l1.chain_admin_addr = deploy_l1_output.deployed_addresses.chain_admin;
        self.ecosystem_contracts.server_notifier_proxy_addr = Some(
            deploy_l1_output
                .deployed_addresses
                .server_notifier_proxy_addr,
        );
    }

    pub fn set_chain_contracts(&mut self, register_chain_output: &RegisterChainOutput) {
        self.l1.diamond_proxy_addr = register_chain_output.diamond_proxy_addr;
        self.l1.governance_addr = register_chain_output.governance_addr;
        self.l1.chain_admin_addr = register_chain_output.chain_admin_addr;
        self.l1.access_control_restriction_addr =
            Some(register_chain_output.access_control_restriction_addr);
        self.l1.chain_proxy_admin_addr = Some(register_chain_output.chain_proxy_admin_addr);
        self.l2.legacy_shared_bridge_addr = register_chain_output.l2_legacy_shared_bridge_addr;
    }

    pub fn set_eth_proof_manager_addresses(
        &mut self,
        impl_addr: String,
        proxy_addr: String,
        proxy_admin_addr: String,
    ) -> anyhow::Result<()> {
        self.proving_network = Some(ProvingNetworkContracts {
            proof_manager_addr: H160::from_str(&impl_addr)?,
            proxy_addr: H160::from_str(&proxy_addr)?,
            proxy_admin_addr: H160::from_str(&proxy_admin_addr)?,
        });

        Ok(())
    }

    pub fn set_l2_shared_bridge(
        &mut self,
        initialize_bridges_output: &InitializeBridgeOutput,
    ) -> anyhow::Result<()> {
        self.bridges.shared.l2_address = Some(L2_ASSET_ROUTER_ADDRESS);
        self.bridges.erc20.l2_address = Some(L2_ASSET_ROUTER_ADDRESS);
        self.l2.l2_native_token_vault_proxy_addr = Some(L2_NATIVE_TOKEN_VAULT_ADDRESS);
        self.l2.da_validator_addr = Some(initialize_bridges_output.l2_da_validator_address);
        Ok(())
    }

    pub fn set_transaction_filterer(&mut self, transaction_filterer_addr: Address) {
        self.l1.transaction_filterer_addr = Some(transaction_filterer_addr);
    }

    pub fn set_consensus_registry(
        &mut self,
        consensus_registry_output: &ConsensusRegistryOutput,
    ) -> anyhow::Result<()> {
        self.l2.consensus_registry = Some(consensus_registry_output.consensus_registry_proxy);
        Ok(())
    }

    pub fn set_default_l2_upgrade(
        &mut self,
        default_upgrade_output: &DefaultL2UpgradeOutput,
    ) -> anyhow::Result<()> {
        self.l2.default_l2_upgrader = default_upgrade_output.l2_default_upgrader;
        Ok(())
    }

    pub fn set_multicall3(&mut self, multicall3_output: &Multicall3Output) -> anyhow::Result<()> {
        self.l2.multicall3 = Some(multicall3_output.multicall3);
        Ok(())
    }

    pub fn set_timestamp_asserter_addr(
        &mut self,
        timestamp_asserter_output: &TimestampAsserterOutput,
    ) -> anyhow::Result<()> {
        self.l2.timestamp_asserter_addr = Some(timestamp_asserter_output.timestamp_asserter);
        Ok(())
    }

    pub fn set_l2_da_validator_address(
        &mut self,
        output: &L2DAValidatorAddressOutput,
    ) -> anyhow::Result<()> {
        self.l2.da_validator_addr = Some(output.l2_da_validator_address);
        Ok(())
    }
}

impl FileConfigWithDefaultName for ContractsConfig {
    const FILE_NAME: &'static str = CONTRACTS_FILE;
}

impl ZkStackConfig for ContractsConfig {}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server_notifier_proxy_addr: Option<Address>,
}

impl ZkStackConfig for EcosystemContracts {}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct BridgesContracts {
    pub erc20: BridgeContractsDefinition,
    pub shared: BridgeContractsDefinition,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l1_nullifier_addr: Option<Address>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct BridgeContractsDefinition {
    pub l1_address: Address,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub l2_address: Option<Address>,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct EthProofManagerContracts {
    pub proof_manager_addr: Address,
    pub proxy_addr: Address,
    pub proxy_admin_addr: Address,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
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
