use ethers::types::{Bytes,Address};
use serde::{Deserialize, Serialize};
use crate::traits::ZkStackConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    pub l2_shared_bridge_implementation: Option<Address>,
    pub l2_shared_bridge_implementation_constructor_data: Option<Bytes>,
    pub l2_shared_bridge_proxy: Option<Address>,
    pub l2_shared_bridge_proxy_constructor_data: Option<Bytes>,
    
    pub l2_force_deploy_upgrader: Option<Address>,
    
    pub l2_consensus_registry_implementation: Option<Address>,
    pub l2_consensus_registry_proxy: Option<Address>,
    pub l2_consensus_registry_proxy_constructor_data: Option<Bytes>,
    
    pub l2_multicall3: Option<Address>,
}

impl ZkStackConfig for Output {}
