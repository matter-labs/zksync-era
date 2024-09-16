use ethers::abi::Address;
use zksync_basic_types::web3::Bytes;
use serde::{Deserialize, Serialize};

use crate::traits::ZkToolboxConfig;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeployGatewayCTMOutput {
    gateway_state_transition: StateTransitionDeployedAddresses,
    diamond_cut_data: Bytes,
}

impl ZkToolboxConfig for DeployGatewayCTMOutput {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StateTransitionDeployedAddresses {
    state_transition_proxy: Address,
    state_transition_implementation: Address,
    verifier: Address,
    admin_facet: Address,
    mailbox_facet: Address,
    executor_facet: Address,
    getters_facet: Address,
    diamond_init: Address,
    genesis_upgrade: Address,
    default_upgrade: Address,
    // The `diamond_proxy` field is removed as indicated by the TODO comment in the Solidity struct.
}