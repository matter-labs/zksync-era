use std::{path::Path, str::FromStr};

use ethers::types::{Address, H256};
use serde::{Deserialize, Serialize};
use xshell::Shell;
use zkstack_cli_common::contracts::encode_ntv_asset_id;
use zksync_basic_types::H160;
use zksync_system_constants::{L2_ASSET_ROUTER_ADDRESS, L2_NATIVE_TOKEN_VAULT_ADDRESS};

use crate::{
    consts::CONTRACTS_FILE,
    forge_interface::{
        deploy_ecosystem::output::{DeployCTMOutput, DeployL1CoreContractsOutput},
        deploy_l2_contracts::output::{
            ConsensusRegistryOutput, DefaultL2UpgradeOutput, InitializeBridgeOutput,
            L2DAValidatorAddressOutput, Multicall3Output, TimestampAsserterOutput,
        },
        register_chain::output::RegisterChainOutput,
    },
    traits::{FileConfigTrait, FileConfigWithDefaultName, ReadConfig},
    ChainConfig,
};

// Contracts related to ecosystem, without chain specifics
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct CoreContractsConfig {
    pub create2_factory_addr: Address,
    pub create2_factory_salt: H256,
    pub multicall3_addr: Address,
    pub core_ecosystem_contracts: CoreEcosystemContracts,
    pub bridges: BridgesContracts,
    pub l1: L1CoreContracts,
    pub era_ctm: Option<ChainTransitionManagerContracts>,
    pub zksync_os_ctm: Option<ChainTransitionManagerContracts>,
    pub proof_manager_contracts: Option<EthProofManagerContracts>,
}

impl CoreContractsConfig {
    pub fn ctm(&self, zksync_os: bool) -> ChainTransitionManagerContracts {
        if zksync_os {
            self.zksync_os_ctm
                .clone()
                .expect("ZkSync OS CTM is not deployed")
        } else {
            self.era_ctm.clone().expect("Era CTM is not deployed")
        }
    }

    // Create full ContractsConfig for a specific chain from the core config and the output of `register_chain`
    pub fn chain_contracts_from_output(
        &self,
        register_chain_output: &RegisterChainOutput,
        chain_config: &ChainConfig,
    ) -> ContractsConfig {
        let ctm = self.ctm(chain_config.zksync_os);
        ContractsConfig {
            create2_factory_addr: self.create2_factory_addr,
            create2_factory_salt: self.create2_factory_salt,
            ecosystem_contracts: EcosystemContracts {
                bridgehub_proxy_addr: self.core_ecosystem_contracts.bridgehub_proxy_addr,
                message_root_proxy_addr: self.core_ecosystem_contracts.message_root_proxy_addr,
                transparent_proxy_admin_addr: self
                    .core_ecosystem_contracts
                    .transparent_proxy_admin_addr,
                stm_deployment_tracker_proxy_addr: self
                    .core_ecosystem_contracts
                    .stm_deployment_tracker_proxy_addr,
                native_token_vault_addr: self.core_ecosystem_contracts.native_token_vault_addr,
                ctm: ctm.clone(),
            },
            bridges: self.bridges.clone(),
            l1: L1Contracts {
                default_upgrade_addr: ctm.default_upgrade_addr,
                diamond_proxy_addr: register_chain_output.diamond_proxy_addr,
                governance_addr: register_chain_output.governance_addr,
                chain_admin_addr: register_chain_output.chain_admin_addr,
                access_control_restriction_addr: Some(
                    register_chain_output.access_control_restriction_addr,
                ),
                chain_proxy_admin_addr: Some(register_chain_output.chain_proxy_admin_addr),
                validator_timelock_addr: ctm.validator_timelock_addr,
                base_token_addr: chain_config.base_token.address,
                rollup_l1_da_validator_addr: Some(ctm.rollup_l1_da_validator_addr),
                transaction_filterer_addr: self.l1.transaction_filterer_addr,
                verifier_addr: ctm.verifier_addr,
                base_token_asset_id: Some(encode_ntv_asset_id(
                    chain_config.l1_network.chain_id().into(),
                    chain_config.base_token.address,
                )),

                multicall3_addr: self.multicall3_addr,
                avail_l1_da_validator_addr: Some(ctm.avail_l1_da_validator_addr),
                no_da_validium_l1_validator_addr: Some(ctm.no_da_validium_l1_validator_addr),
            },
            proof_manager_contracts: self.proof_manager_contracts.clone(),
            // L2 fields will be set later, after L2 deployment
            l2: L2Contracts {
                legacy_shared_bridge_addr: register_chain_output.l2_legacy_shared_bridge_addr,
                ..Default::default()
            },
            other: Default::default(),
        }
    }

    pub fn set_eth_proof_manager_addresses(
        &mut self,
        impl_addr: String,
        proxy_addr: String,
        proxy_admin_addr: String,
    ) -> anyhow::Result<()> {
        self.proof_manager_contracts = Some(EthProofManagerContracts {
            proof_manager_addr: H160::from_str(&impl_addr)?,
            proxy_addr: H160::from_str(&proxy_addr)?,
            proxy_admin_addr: H160::from_str(&proxy_admin_addr)?,
        });

        Ok(())
    }

    pub fn read_with_fallback(
        shell: &Shell,
        path: impl AsRef<Path>,
        zksync_os: bool,
    ) -> anyhow::Result<CoreContractsConfig> {
        match CoreContractsConfig::read(shell, &path) {
            Ok(config) => Ok(config),
            Err(_) => {
                let old_config: ContractsConfig = ContractsConfig::read(shell, path)?;
                Ok(CoreContractsConfig::from_chain_contracts(
                    old_config, zksync_os,
                ))
            }
        }
    }
}

impl CoreContractsConfig {
    pub fn from_chain_contracts(
        chain_contracts: ContractsConfig,
        zksync_os: bool,
    ) -> CoreContractsConfig {
        let mut contracts = CoreContractsConfig {
            create2_factory_addr: chain_contracts.create2_factory_addr,
            create2_factory_salt: chain_contracts.create2_factory_salt,
            multicall3_addr: chain_contracts.l1.multicall3_addr,
            core_ecosystem_contracts: CoreEcosystemContracts {
                bridgehub_proxy_addr: chain_contracts.ecosystem_contracts.bridgehub_proxy_addr,
                message_root_proxy_addr: chain_contracts
                    .ecosystem_contracts
                    .message_root_proxy_addr,
                transparent_proxy_admin_addr: chain_contracts
                    .ecosystem_contracts
                    .transparent_proxy_admin_addr,
                stm_deployment_tracker_proxy_addr: chain_contracts
                    .ecosystem_contracts
                    .stm_deployment_tracker_proxy_addr,
                native_token_vault_addr: chain_contracts
                    .ecosystem_contracts
                    .native_token_vault_addr,
            },
            bridges: chain_contracts.bridges,
            l1: L1CoreContracts {
                governance_addr: chain_contracts.l1.governance_addr,
                chain_admin_addr: chain_contracts.l1.chain_admin_addr,
                access_control_restriction_addr: chain_contracts.l1.access_control_restriction_addr,
                chain_proxy_admin_addr: chain_contracts.l1.chain_proxy_admin_addr,
                transaction_filterer_addr: chain_contracts.l1.transaction_filterer_addr,
            },
            era_ctm: None,
            zksync_os_ctm: None,
            proof_manager_contracts: chain_contracts.proof_manager_contracts,
        };
        if zksync_os {
            contracts.zksync_os_ctm = Some(chain_contracts.ecosystem_contracts.ctm);
        } else {
            contracts.era_ctm = Some(chain_contracts.ecosystem_contracts.ctm);
        }
        contracts
    }

    pub fn update_from_l1_output(
        &mut self,
        deploy_l1_core_contracts_output: &DeployL1CoreContractsOutput,
    ) {
        self.create2_factory_addr = deploy_l1_core_contracts_output.create2_factory_addr;
        self.create2_factory_salt = deploy_l1_core_contracts_output.create2_factory_salt;
        self.bridges.erc20.l1_address = deploy_l1_core_contracts_output
            .deployed_addresses
            .bridges
            .erc20_bridge_proxy_addr;
        self.bridges.shared.l1_address = deploy_l1_core_contracts_output
            .deployed_addresses
            .bridges
            .shared_bridge_proxy_addr;
        self.bridges.l1_nullifier_addr = Some(
            deploy_l1_core_contracts_output
                .deployed_addresses
                .bridges
                .l1_nullifier_proxy_addr,
        );
        self.core_ecosystem_contracts.bridgehub_proxy_addr = deploy_l1_core_contracts_output
            .deployed_addresses
            .bridgehub
            .bridgehub_proxy_addr;
        self.core_ecosystem_contracts.message_root_proxy_addr = Some(
            deploy_l1_core_contracts_output
                .deployed_addresses
                .bridgehub
                .message_root_proxy_addr,
        );
        self.core_ecosystem_contracts.transparent_proxy_admin_addr =
            deploy_l1_core_contracts_output
                .deployed_addresses
                .transparent_proxy_admin_addr;
        self.core_ecosystem_contracts
            .stm_deployment_tracker_proxy_addr = Some(
            deploy_l1_core_contracts_output
                .deployed_addresses
                .bridgehub
                .ctm_deployment_tracker_proxy_addr,
        );

        self.l1.governance_addr = deploy_l1_core_contracts_output
            .deployed_addresses
            .governance_addr;
        self.core_ecosystem_contracts.native_token_vault_addr = Some(
            deploy_l1_core_contracts_output
                .deployed_addresses
                .native_token_vault_addr,
        );
        self.l1.chain_admin_addr = deploy_l1_core_contracts_output
            .deployed_addresses
            .chain_admin;
    }

    // Update CTM related fields from the output of CTM deployment
    pub fn update_from_ctm_output(&mut self, deploy_ctm_output: &DeployCTMOutput, zksync_os: bool) {
        let ctm = ChainTransitionManagerContracts {
            state_transition_proxy_addr: deploy_ctm_output
                .deployed_addresses
                .state_transition
                .state_transition_proxy_addr,
            validator_timelock_addr: deploy_ctm_output.deployed_addresses.validator_timelock_addr,
            diamond_cut_data: deploy_ctm_output.contracts_config.diamond_cut_data.clone(),
            force_deployments_data: deploy_ctm_output
                .contracts_config
                .force_deployments_data
                .clone(),
            l1_bytecodes_supplier_addr: deploy_ctm_output
                .deployed_addresses
                .state_transition
                .bytecodes_supplier_addr,
            expected_rollup_l2_da_validator: deploy_ctm_output.expected_rollup_l2_da_validator_addr,
            l1_wrapped_base_token_store: None,
            server_notifier_proxy_addr: deploy_ctm_output
                .deployed_addresses
                .server_notifier_proxy_addr,
            default_upgrade_addr: deploy_ctm_output
                .deployed_addresses
                .state_transition
                .default_upgrade_addr,
            genesis_upgrade_addr: deploy_ctm_output
                .deployed_addresses
                .state_transition
                .genesis_upgrade_addr,
            verifier_addr: deploy_ctm_output
                .deployed_addresses
                .state_transition
                .verifier_addr,
            chain_admin: deploy_ctm_output.deployed_addresses.chain_admin,
            proxy_admin: deploy_ctm_output
                .deployed_addresses
                .transparent_proxy_admin_addr,
            governance: deploy_ctm_output.deployed_addresses.governance_addr,

            rollup_l1_da_validator_addr: deploy_ctm_output
                .deployed_addresses
                .rollup_l1_da_validator_addr,
            no_da_validium_l1_validator_addr: deploy_ctm_output
                .deployed_addresses
                .no_da_validium_l1_validator_addr,
            avail_l1_da_validator_addr: deploy_ctm_output
                .deployed_addresses
                .avail_l1_da_validator_addr,
            l1_rollup_da_manager: deploy_ctm_output.deployed_addresses.l1_rollup_da_manager,
        };
        self.multicall3_addr = deploy_ctm_output.multicall3_addr;
        if zksync_os {
            self.zksync_os_ctm = Some(ctm);
        } else {
            self.era_ctm = Some(ctm);
        }
    }
}

impl FileConfigWithDefaultName for CoreContractsConfig {
    const FILE_NAME: &'static str = CONTRACTS_FILE;
}

impl FileConfigTrait for CoreContractsConfig {}

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
    pub fn set_eth_proof_manager_addresses(
        &mut self,
        impl_addr: String,
        proxy_addr: String,
        proxy_admin_addr: String,
    ) -> anyhow::Result<()> {
        self.proof_manager_contracts = Some(EthProofManagerContracts {
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

impl FileConfigTrait for ContractsConfig {}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct ContractsConfigForDeployERC20 {
    pub create2_factory_addr: Address,
    pub create2_factory_salt: H256,
}

impl From<CoreContractsConfig> for ContractsConfigForDeployERC20 {
    fn from(config: CoreContractsConfig) -> Self {
        ContractsConfigForDeployERC20 {
            create2_factory_addr: config.create2_factory_addr,
            create2_factory_salt: config.create2_factory_salt,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct CoreEcosystemContracts {
    pub bridgehub_proxy_addr: Address,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_root_proxy_addr: Option<Address>,
    pub transparent_proxy_admin_addr: Address,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stm_deployment_tracker_proxy_addr: Option<Address>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_token_vault_addr: Option<Address>,
}

/// All contracts related to Chain Transition Manager (CTM)
/// This contracts are deployed only once per CTM, ecosystem can have multiple CTMs
#[derive(Debug, Deserialize, Serialize, Clone, Default, PartialEq)]
pub struct ChainTransitionManagerContracts {
    pub governance: Address,
    pub chain_admin: Address,
    pub proxy_admin: Address,
    pub state_transition_proxy_addr: Address,
    pub validator_timelock_addr: Address,
    pub diamond_cut_data: String,
    pub force_deployments_data: Option<String>,
    pub l1_bytecodes_supplier_addr: Address,
    pub expected_rollup_l2_da_validator: Address,
    pub l1_wrapped_base_token_store: Option<Address>,
    pub server_notifier_proxy_addr: Address,
    pub default_upgrade_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub verifier_addr: Address,
    pub rollup_l1_da_validator_addr: Address,
    pub no_da_validium_l1_validator_addr: Address,
    pub avail_l1_da_validator_addr: Address,
    pub l1_rollup_da_manager: Address,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Default)]
pub struct EcosystemContracts {
    pub bridgehub_proxy_addr: Address,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_root_proxy_addr: Option<Address>,
    pub transparent_proxy_admin_addr: Address,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stm_deployment_tracker_proxy_addr: Option<Address>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_token_vault_addr: Option<Address>,
    #[serde(flatten)]
    pub ctm: ChainTransitionManagerContracts,
}

impl FileConfigTrait for EcosystemContracts {}

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
pub struct L1CoreContracts {
    pub governance_addr: Address,
    #[serde(default)]
    pub chain_admin_addr: Address,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub access_control_restriction_addr: Option<Address>,
    // `Option` to be able to parse configs from pre-gateway protocol version.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chain_proxy_admin_addr: Option<Address>,
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
