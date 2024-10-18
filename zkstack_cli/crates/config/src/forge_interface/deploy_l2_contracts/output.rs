use ethers::types::Address;
use serde::{Deserialize, Serialize};

use crate::traits::ZkStackConfig;

impl ZkStackConfig for InitializeBridgeOutput {}
impl ZkStackConfig for DefaultL2UpgradeOutput {}
impl ZkStackConfig for ConsensusRegistryOutput {}
impl ZkStackConfig for Multicall3Output {}

impl ZkStackConfig for TimestampAsserterOutput {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeBridgeOutput {
    pub l2_shared_bridge_implementation: Address,
    pub l2_shared_bridge_proxy: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DefaultL2UpgradeOutput {
    pub l2_default_upgrader: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConsensusRegistryOutput {
    pub consensus_registry_implementation: Address,
    pub consensus_registry_proxy: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Multicall3Output {
    pub multicall3: Address,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampAsserterOutput {
    pub timestamp_asserter: Address,
}
