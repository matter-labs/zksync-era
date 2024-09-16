use ethers::abi::Address;
use serde::{Deserialize, Serialize};
use zksync_basic_types::web3::Bytes;

use crate::traits::ZkToolboxConfig;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeployGatewayCTMOutput {
    pub gateway_state_transition: StateTransitionDeployedAddresses,
    pub multicall3_addr: Address,
    pub validium_da_validator: Address,
    pub relayed_sl_da_validator: Address,
    pub diamond_cut_data: Bytes,
}

impl ZkToolboxConfig for DeployGatewayCTMOutput {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StateTransitionDeployedAddresses {
    pub state_transition_proxy_addr: Address,
    pub state_transition_implementation_addr: Address,
    pub verifier_addr: Address,
    pub admin_facet_addr: Address,
    pub mailbox_facet_addr: Address,
    pub executor_facet_addr: Address,
    pub getters_facet_addr: Address,
    pub diamond_init_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub default_upgrade_addr: Address,
    pub validator_timelock_addr: Address,
    // The `diamond_proxy` field is removed as indicated by the TODO comment in the Solidity struct.
}
