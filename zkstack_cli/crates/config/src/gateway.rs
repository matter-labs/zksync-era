use ethers::utils::hex;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{web3::Bytes, Address, SLChainId};

use crate::{
    forge_interface::deploy_gateway_ctm::output::DeployGatewayCTMOutput,
    raw::PatchedConfig,
    traits::{FileConfigWithDefaultName, ZkStackConfig},
    GATEWAY_FILE,
};

/// Config that is only stored for the gateway chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GatewayConfig {
    pub state_transition_proxy_addr: Address,
    pub state_transition_implementation_addr: Address,
    pub verifier_addr: Address,
    pub validator_timelock_addr: Address,
    pub admin_facet_addr: Address,
    pub mailbox_facet_addr: Address,
    pub executor_facet_addr: Address,
    pub getters_facet_addr: Address,
    pub diamond_init_addr: Address,
    pub genesis_upgrade_addr: Address,
    pub default_upgrade_addr: Address,
    pub multicall3_addr: Address,
    pub relayed_sl_da_validator: Address,
    pub validium_da_validator: Address,
    pub diamond_cut_data: Bytes,
}

impl FileConfigWithDefaultName for GatewayConfig {
    const FILE_NAME: &'static str = GATEWAY_FILE;
}

impl ZkStackConfig for GatewayConfig {}

impl From<DeployGatewayCTMOutput> for GatewayConfig {
    fn from(output: DeployGatewayCTMOutput) -> Self {
        GatewayConfig {
            state_transition_proxy_addr: output
                .gateway_state_transition
                .chain_type_manager_proxy_addr,
            state_transition_implementation_addr: output
                .gateway_state_transition
                .chain_type_manager_implementation_addr,
            verifier_addr: output.gateway_state_transition.verifier_addr,
            admin_facet_addr: output.gateway_state_transition.admin_facet_addr,
            mailbox_facet_addr: output.gateway_state_transition.mailbox_facet_addr,
            executor_facet_addr: output.gateway_state_transition.executor_facet_addr,
            getters_facet_addr: output.gateway_state_transition.getters_facet_addr,
            diamond_init_addr: output.gateway_state_transition.diamond_init_addr,
            genesis_upgrade_addr: output.gateway_state_transition.genesis_upgrade_addr,
            default_upgrade_addr: output.gateway_state_transition.default_upgrade_addr,
            multicall3_addr: output.multicall3_addr,
            diamond_cut_data: hex::decode(output.diamond_cut_data.clone()).unwrap().into(),
            validator_timelock_addr: output.gateway_state_transition.validator_timelock_addr,
            relayed_sl_da_validator: output.relayed_sl_da_validator,
            validium_da_validator: output.validium_da_validator,
        }
    }
}

pub fn init_gateway_chain_config(
    config: &mut PatchedConfig,
    gateway_config: &GatewayConfig,
    diamond_proxy_addr: Address,
    l2_chain_admin_addr: Address,
    gateway_chain_id: SLChainId,
) -> anyhow::Result<()> {
    config.insert_yaml(
        "state_transition_proxy_addr",
        gateway_config.state_transition_proxy_addr,
    )?;
    config.insert_yaml(
        "validator_timelock_addr",
        gateway_config.validator_timelock_addr,
    )?;
    config.insert_yaml("multicall3_addr", gateway_config.multicall3_addr)?;
    config.insert_yaml("diamond_proxy_addr", diamond_proxy_addr)?;
    config.insert_yaml("chain_admin_addr", l2_chain_admin_addr)?;
    config.insert_yaml("governance_addr", l2_chain_admin_addr)?;
    config.insert_yaml("gateway_chain_id", gateway_chain_id)?;
    Ok(())
}
