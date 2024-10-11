use ethers::types::Address;
use serde::{Deserialize, Serialize};
use zksync_basic_types::L2ChainId;

use crate::{traits::ZkToolboxConfig, ChainConfig, EcosystemConfig};

impl ZkToolboxConfig for DeployL2ContractsInput {}

/// Fields corresponding to `contracts/l1-contracts/deploy-script-config-template/config-deploy-l2-config.toml`
/// which are read by `contracts/l1-contracts/deploy-scripts/DeployL2Contracts.sol`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployL2ContractsInput {
    pub era_chain_id: L2ChainId,
    pub chain_id: L2ChainId,
    pub l1_shared_bridge: Address,
    pub bridgehub: Address,
    pub governance: Address,
    pub erc20_bridge: Address,
    pub consensus_registry_owner: Address,
}

impl DeployL2ContractsInput {
    pub fn new(
            ecosystem_config: &EcosystemConfig,
            chain_config: &ChainConfig,
        ) -> anyhow::Result<Self> {
        let contracts = ecosystem_config.get_contracts_config()?;
        Ok(Self {
            era_chain_id: ecosystem_config.era_chain_id,
            chain_id: chain_config.chain_id,
            l1_shared_bridge: contracts.bridges.shared.l1_address,
            bridgehub: contracts.ecosystem_contracts.bridgehub_proxy_addr,
            governance: contracts.l1.governance_addr,
            erc20_bridge: contracts.bridges.erc20.l1_address,
            // FIXME: check whether it should be aliased.
            consensus_registry_owner: contracts.l1.governance_addr,
        })
    }
}
