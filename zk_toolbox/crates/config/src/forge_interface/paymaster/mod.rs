use ethers::types::Address;
use serde::{Deserialize, Serialize};
use types::ChainId;

use crate::{traits::ZkToolboxConfig, ChainConfig};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeployPaymasterInput {
    pub chain_id: ChainId,
    pub l1_shared_bridge: Address,
    pub bridgehub: Address,
}

impl DeployPaymasterInput {
    pub fn new(chain_config: &ChainConfig) -> anyhow::Result<Self> {
        let contracts = chain_config.get_contracts_config()?;
        Ok(Self {
            chain_id: chain_config.chain_id,
            l1_shared_bridge: contracts.bridges.shared.l1_address,
            bridgehub: contracts.ecosystem_contracts.bridgehub_proxy_addr,
        })
    }
}

impl ZkToolboxConfig for DeployPaymasterInput {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeployPaymasterOutput {
    pub paymaster: Address,
}

impl ZkToolboxConfig for DeployPaymasterOutput {}
