use ethers::addressbook::Address;
use serde::{Deserialize, Serialize};

use crate::{
    configs::{HyperchainConfig, ReadConfig, SaveConfig},
    types::ChainId,
};

impl ReadConfig for InitializeBridgeInput {}
impl SaveConfig for InitializeBridgeInput {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitializeBridgeInput {
    pub era_chain_id: ChainId,
    pub chain_id: ChainId,
    pub l1_shared_bridge: Address,
    pub bridgehub: Address,
    pub governance: Address,
    pub erc20_bridge: Address,
}

impl InitializeBridgeInput {
    pub fn new(
        hyperchain_config: &HyperchainConfig,
        era_chain_id: ChainId,
    ) -> anyhow::Result<Self> {
        let contracts = hyperchain_config.get_contracts_config()?;
        let wallets = hyperchain_config.get_wallets_config()?;
        Ok(Self {
            era_chain_id,
            chain_id: hyperchain_config.chain_id,
            l1_shared_bridge: contracts.bridges.shared.l1_address,
            bridgehub: contracts.ecosystem_contracts.bridgehub_proxy_addr,
            governance: wallets.governor.address,
            erc20_bridge: contracts.bridges.erc20.l1_address,
        })
    }
}
