use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::FileConfigTrait;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployCTMOutput {
    pub contracts_config: DeployCTMContractsConfigOutput,
    pub deployed_addresses: DeployCTMDeployedAddressesOutput,
    pub multicall3_addr: Address,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployCTMDeployedAddressesOutput {
    pub governance_addr: Address,
    pub transparent_proxy_admin_addr: Address,
    pub validator_timelock_addr: Address,
    pub chain_admin: Address,
    pub state_transition: L1StateTransitionOutput,
    pub rollup_l1_da_validator_addr: Address,
    pub no_da_validium_l1_validator_addr: Address,
    pub avail_l1_da_validator_addr: Address,
    pub l1_rollup_da_manager: Address,
    pub blobs_zksync_os_l1_da_validator_addr: Option<Address>,
    pub server_notifier_proxy_addr: Address,
}

impl FileConfigTrait for DeployCTMOutput {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct DeployCTMContractsConfigOutput {
    pub diamond_cut_data: String,
    pub force_deployments_data: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct L1StateTransitionOutput {
    pub state_transition_proxy_addr: Address,
    pub verifier_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub default_upgrade_addr: Address,
    pub bytecodes_supplier_addr: Address,
}
