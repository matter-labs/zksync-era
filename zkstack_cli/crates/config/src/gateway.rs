use ethers::utils::hex;
use zksync_config::configs::{contracts::gateway::GatewayConfig, gateway::GatewayChainConfig};

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
