use ethers::types::Address;
use serde::{Deserialize, Serialize};
use zksync_basic_types::L2ChainId;
use crate::{traits::ZkStackConfig,ChainConfig};

/// Fields corresponding to `contracts/l1-contracts/deploy-script-config-template/config-deploy-l2-config.toml`
/// which are read by `contracts/l1-contracts/deploy-scripts/DeployL2Contracts.sol`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Input {
    pub bridgehub: Address,
    pub l1_shared_bridge: Address,
    pub governance: Address,
    pub erc20_bridge: Address,
    pub consensus_registry_owner: Address,
    pub chain_id: L2ChainId,
    pub era_chain_id: L2ChainId,
    pub legacy_bridge: bool,

    pub deploy_shared_bridge: bool,
    pub deploy_consensus_registry: bool,
    pub deploy_multicall3: bool,
    pub deploy_force_deploy_upgrader: bool,
}

impl ZkStackConfig for Input {}

impl Input {
    pub fn new(chain_config: &ChainConfig, era_chain_id: L2ChainId) -> anyhow::Result<Self> {
        let contracts = chain_config.get_contracts_config()?;
        let wallets = chain_config.get_wallets_config()?;
        Ok(Self {
            era_chain_id,
            chain_id: chain_config.chain_id,
            l1_shared_bridge: contracts.bridges.shared.l1_address,
            bridgehub: contracts.ecosystem_contracts.bridgehub_proxy_addr,
            governance: wallets.governor.address,
            erc20_bridge: contracts.bridges.erc20.l1_address,
            consensus_registry_owner: wallets.governor.address,
            legacy_bridge: chain_config.legacy_bridge.unwrap_or(false),

            deploy_shared_bridge: false,
            deploy_consensus_registry: false,
            deploy_multicall3: false,
            deploy_force_deploy_upgrader: false,
        })
    }
}
