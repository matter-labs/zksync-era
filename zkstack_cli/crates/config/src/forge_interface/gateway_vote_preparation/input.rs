use ethers::types::{Address, H256, U256};
use serde::{Deserialize, Serialize};

use crate::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig, traits::FileConfigTrait,
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
    pub is_zk_sync_os: bool,
    pub contracts: GatewayContractsConfig,
    pub tokens: TokensConfig,
    pub refund_recipient: Address,
    pub gateway_chain_id: U256,
    pub force_deployments_data: String,
}

impl FileConfigTrait for GatewayVotePreparationConfig {}

impl GatewayVotePreparationConfig {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        initial_deployment_config: &InitialDeploymentConfig,
        external_contracts_config: &ContractsConfig,
        era_chain_id: U256,
        gateway_chain_id: U256,
        owner_address: Address,
        testnet_verifier: bool,
        is_zk_sync_os: bool,
        refund_recipient: Address,
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
            is_zk_sync_os,
            contracts,
            tokens,
            refund_recipient,
            gateway_chain_id,
            force_deployments_data: external_contracts_config
                .ecosystem_contracts
                .ctm
                .force_deployments_data
                .clone()
                .unwrap_or_default(),
        }
    }
}
