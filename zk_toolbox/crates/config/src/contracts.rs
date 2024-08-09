use std::ops::Add;

use ethers::types::{Address, H256};
use serde::{Deserialize, Serialize};
use zksync_basic_types::{web3::Bytes, H160};

use crate::{
    consts::CONTRACTS_FILE,
    forge_interface::{
        deploy_ecosystem::output::DeployL1Output,
        deploy_l2_contracts::output::{DefaultL2UpgradeOutput, InitializeBridgeOutput},
        register_chain::output::RegisterChainOutput,
    },
    traits::{FileConfigWithDefaultName, ZkToolboxConfig},
};

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ContractsConfig {
    pub create2_factory_addr: Address,
    pub create2_factory_salt: H256,
    pub ecosystem_contracts: EcosystemContracts,
    pub bridges: BridgesContracts,
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
        self.ecosystem_contracts.native_token_vault_addr = deploy_l1_output.deployed_addresses.native_token_vault_addr;
        self.l1.verifier_addr = deploy_l1_output
            .deployed_addresses
            .state_transition
            .verifier_addr;
        self.l1.validator_timelock_addr =
            deploy_l1_output.deployed_addresses.validator_timelock_addr;
        self.ecosystem_contracts
            .diamond_cut_data
            .clone_from(&deploy_l1_output.contracts_config.diamond_cut_data);
        self.l1.rollup_l1_da_validator_addr = deploy_l1_output.deployed_addresses.rollup_l1_da_validator_addr;
        self.l1.validium_l1_da_validator_addr = deploy_l1_output.deployed_addresses.validium_l1_da_validator_addr;
        self.l1.force_deployments_data = deploy_l1_output.contracts_config.force_deployments_data.clone();
    }

    pub fn set_chain_contracts(&mut self, register_chain_output: &RegisterChainOutput) {
        self.l1.diamond_proxy_addr = register_chain_output.diamond_proxy_addr;
        self.l1.governance_addr = register_chain_output.governance_addr;
        self.l1.chain_admin_addr = register_chain_output.chain_admin_addr;
    }

    pub fn set_l2_shared_bridge(
        &mut self,
        initialize_bridges_output: &InitializeBridgeOutput,
    ) -> anyhow::Result<()> {
        self.bridges.shared.l2_address = Some(initialize_bridges_output.l2_shared_bridge_proxy);
        self.bridges.erc20.l2_address = Some(initialize_bridges_output.l2_shared_bridge_proxy);
        self.l2.da_validator_addr = initialize_bridges_output.l2_da_validator_addr;

        // FIXME: delete this variable
        self.l2.native_token_vault_addr = H160([
            0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x01, 0x00, 0x04,
        ]);
        Ok(())
    }

    pub fn set_default_l2_upgrade(
        &mut self,
        default_upgrade_output: &DefaultL2UpgradeOutput,
    ) -> anyhow::Result<()> {
        self.l2.default_l2_upgrader = default_upgrade_output.l2_default_upgrader;
        Ok(())
    }
}

impl FileConfigWithDefaultName for ContractsConfig {
    const FILE_NAME: &'static str = CONTRACTS_FILE;
}

impl ZkToolboxConfig for ContractsConfig {}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct EcosystemContracts {
    pub bridgehub_proxy_addr: Address,
    pub state_transition_proxy_addr: Address,
    pub transparent_proxy_admin_addr: Address,
    pub validator_timelock_addr: Address,
    pub diamond_cut_data: String,
    pub native_token_vault_addr: Address
}

impl ZkToolboxConfig for EcosystemContracts {}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct BridgesContracts {
    pub erc20: BridgeContractsDefinition,
    pub shared: BridgeContractsDefinition,
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
    pub multicall3_addr: Address,
    pub verifier_addr: Address,
    pub validator_timelock_addr: Address,
    pub base_token_addr: Address,
    pub rollup_l1_da_validator_addr: Address,
    pub validium_l1_da_validator_addr: Address,
    pub force_deployments_data: Bytes,
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct L2Contracts {
    pub testnet_paymaster_addr: Address,
    pub default_l2_upgrader: Address,
    pub da_validator_addr: Address,
    pub native_token_vault_addr: Address
}
