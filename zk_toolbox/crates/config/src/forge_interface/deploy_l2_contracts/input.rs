use ethers::types::Address;
use serde::{Deserialize, Serialize};
use types::L1BatchCommitmentMode;
use zksync_basic_types::L2ChainId;

use crate::{traits::ZkToolboxConfig, ChainConfig};

impl ZkToolboxConfig for DeployL2ContractsInput {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployL2ContractsInput {
    pub era_chain_id: L2ChainId,
    pub chain_id: L2ChainId,
    pub l1_shared_bridge: Address,
    pub bridgehub: Address,
    pub governance: Address,
    pub erc20_bridge: Address,
    pub diamond_proxy_addr: Address,
    pub validium_mode: bool,
    pub l1_da_validator_addr: Address,
    pub chain_admin: Address,
}

impl DeployL2ContractsInput {
    pub fn new(chain_config: &ChainConfig, era_chain_id: L2ChainId) -> anyhow::Result<Self> {
        let contracts = chain_config.get_contracts_config()?;
        let wallets = chain_config.get_wallets_config()?;

        let validium_mode = chain_config.l1_batch_commit_data_generator_mode
            == L1BatchCommitmentMode::Validium;

        let l1_da_validator_addr = if validium_mode {
            contracts.l1.rollup_l1_da_validator_addr
        } else {
            contracts.l1.validium_l1_da_validator_addr
        };

        Ok(Self {
            era_chain_id,
            chain_id: chain_config.chain_id,
            l1_shared_bridge: contracts.bridges.shared.l1_address,
            bridgehub: contracts.ecosystem_contracts.bridgehub_proxy_addr,
            governance: wallets.governor.address,
            erc20_bridge: contracts.bridges.erc20.l1_address,
            diamond_proxy_addr: contracts.l1.diamond_proxy_addr,
            validium_mode,
            l1_da_validator_addr,
            chain_admin: contracts.l1.chain_admin_addr,
        })
    }
}
