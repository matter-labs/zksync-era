use ethers::utils::hex;
use zksync_config::configs::{contracts::gateway::GatewayConfig, gateway::GatewayChainConfig};

use crate::{
    forge_interface::gateway_vote_preparation::output::DeployGatewayCTMOutput,
    traits::{FileConfigWithDefaultName, ZkStackConfig},
    GATEWAY_CHAIN_FILE, GATEWAY_FILE,
};

impl FileConfigWithDefaultName for GatewayConfig {
    const FILE_NAME: &'static str = GATEWAY_FILE;
}

impl ZkStackConfig for GatewayConfig {}

impl FileConfigWithDefaultName for GatewayChainConfig {
    const FILE_NAME: &'static str = GATEWAY_CHAIN_FILE;
}

impl ZkStackConfig for GatewayChainConfig {}
