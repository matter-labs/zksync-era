use ethers::types::Address;
use serde::{Deserialize, Serialize};
use zksync_basic_types::L2ChainId;

use crate::{traits::FileConfigTrait, ChainConfig};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeployPaymasterInput {
    pub chain_id: L2ChainId,
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

impl FileConfigTrait for DeployPaymasterInput {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DeployPaymasterOutput {
    pub paymaster: Address,
}

impl FileConfigTrait for DeployPaymasterOutput {}
