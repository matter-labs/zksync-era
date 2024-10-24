use crate::traits::ZkStackConfig;
use common::contracts::ContractSpec;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Output {
    pub l2_shared_bridge_implementation: Option<ContractSpec>,
    pub l2_shared_bridge_proxy: Option<ContractSpec>,
    pub l2_force_deploy_upgrader: Option<ContractSpec>,
    pub l2_consensus_registry_implementation: Option<ContractSpec>,
    pub l2_consensus_registry_proxy: Option<ContractSpec>,
    pub l2_multicall3: Option<ContractSpec>,
}

impl ZkStackConfig for Output {}
