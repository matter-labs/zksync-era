use anyhow::Context;
use ethers::utils::hex;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{web3::Bytes, Address};

use crate::{gateway::GatewayConfig, traits::FileConfigTrait, ChainConfig, ContractsConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayPreparationConfig {
    pub bridgehub_proxy_addr: Address,
    pub ctm_deployment_tracker_proxy_addr: Address,
    pub chain_type_manager_proxy_addr: Address,
    pub shared_bridge_proxy_addr: Address,
    pub governance: Address,
    pub chain_chain_id: u64, // Assuming uint256 can be represented as u64 for chain ID, use U256 for full uint256 support
    pub gateway_diamond_cut_data: Bytes,
    pub l1_diamond_cut_data: Bytes,
    pub chain_proxy_admin: Address,
    pub chain_admin: Address,
    pub access_control_restriction: Address,
    pub l1_nullifier_proxy_addr: Address,
}

impl FileConfigTrait for GatewayPreparationConfig {}

impl GatewayPreparationConfig {
    pub fn new(
        chain_config: &ChainConfig,
        chain_contracts_config: &ContractsConfig,
        ecosystem_contracts_config: &ContractsConfig,
        gateway_config: &GatewayConfig,
    ) -> anyhow::Result<Self> {
        let contracts = chain_config.get_contracts_config()?;

        Ok(Self {
            bridgehub_proxy_addr: contracts.ecosystem_contracts.bridgehub_proxy_addr,
            chain_chain_id: chain_config.chain_id.as_u64(),
            ctm_deployment_tracker_proxy_addr: contracts
                .ecosystem_contracts
                .stm_deployment_tracker_proxy_addr
                .context("stm_deployment_tracker_proxy_addr")?,
            chain_type_manager_proxy_addr: contracts
                .ecosystem_contracts
                .state_transition_proxy_addr,
            shared_bridge_proxy_addr: contracts.bridges.shared.l1_address,
            governance: ecosystem_contracts_config.l1.governance_addr,
            gateway_diamond_cut_data: gateway_config.diamond_cut_data.clone(),
            chain_proxy_admin: chain_contracts_config
                .l1
                .chain_proxy_admin_addr
                .context("chain_proxy_admin_addr")?,
            chain_admin: chain_contracts_config.l1.chain_admin_addr,
            access_control_restriction: chain_contracts_config
                .l1
                .access_control_restriction_addr
                .context("access_control_restriction_addr")?,
            l1_nullifier_proxy_addr: chain_contracts_config
                .bridges
                .l1_nullifier_addr
                .context("l1_nullifier_addr")?,
            l1_diamond_cut_data: hex::decode(
                &chain_contracts_config.ecosystem_contracts.diamond_cut_data,
            )
            .context("diamond_cut_data")?
            .into(),
        })
    }
}
