use ethers::utils::hex;
use zksync_config::configs::{gateway::GatewayChainConfig, GatewayConfig};

use crate::{
    forge_interface::deploy_gateway_ctm::output::DeployGatewayCTMOutput,
    traits::{FileConfigWithDefaultName, ZkStackConfig},
    GATEWAY_CHAIN_FILE, GATEWAY_FILE,
};

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

impl FileConfigWithDefaultName for GatewayChainConfig {
    const FILE_NAME: &'static str = GATEWAY_CHAIN_FILE;
}

impl ZkStackConfig for GatewayChainConfig {}
