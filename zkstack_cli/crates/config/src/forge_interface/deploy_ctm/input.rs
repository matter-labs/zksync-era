use ethers::types::{Address, H256};
use serde::{Deserialize, Serialize};
use zkstack_cli_types::{L1Network, VMOption};

use crate::{
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig, traits::FileConfigTrait,
    WalletsConfig,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployCTMConfig {
    pub owner_address: Address,
    pub testnet_verifier: bool,
    pub support_l2_legacy_shared_bridge_test: bool,
    pub contracts: ContractsDeployCTMConfig,
    pub is_zk_sync_os: bool,
}

impl FileConfigTrait for DeployCTMConfig {}

impl DeployCTMConfig {
    pub fn new(
        wallets_config: &WalletsConfig,
        initial_deployment_config: &InitialDeploymentConfig,
        testnet_verifier: bool,
        l1_network: L1Network,
        support_l2_legacy_shared_bridge_test: bool,
        vm_option: VMOption,
    ) -> Self {
        Self {
            is_zk_sync_os: vm_option.is_zksync_os(),
            testnet_verifier,
            owner_address: wallets_config.governor.address,
            support_l2_legacy_shared_bridge_test,
            contracts: ContractsDeployCTMConfig {
                create2_factory_addr: initial_deployment_config.create2_factory_addr,
                create2_factory_salt: initial_deployment_config.create2_factory_salt,
                // TODO verify correctnesss
                governance_security_council_address: wallets_config.governor.address,
                governance_min_delay: initial_deployment_config.governance_min_delay,
                validator_timelock_execution_delay: initial_deployment_config
                    .validator_timelock_execution_delay,
                avail_l1_da_validator_addr: l1_network.avail_l1_da_validator_addr(),
            },
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ContractsDeployCTMConfig {
    pub create2_factory_salt: H256,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub create2_factory_addr: Option<Address>,
    pub governance_security_council_address: Address,
    pub governance_min_delay: u64,
    pub validator_timelock_execution_delay: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avail_l1_da_validator_addr: Option<Address>,
}
