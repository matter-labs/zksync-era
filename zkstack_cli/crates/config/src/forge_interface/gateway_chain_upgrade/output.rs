use std::{collections::HashMap, str::FromStr};

use ethers::{
    prelude::U256,
    types::{Address, H256},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use zksync_basic_types::L2ChainId;

use crate::{
    apply_l1_to_l2_alias,
    consts::INITIAL_DEPLOYMENT_FILE,
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig,
    traits::{FileConfigWithDefaultName, ZkStackConfig},
    ContractsConfig, GenesisConfig, WalletsConfig, ERC20_DEPLOYMENT_FILE,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GatewayChainUpgradeOutput {
    // This should be the address that controls the current `ChainAdmin`
    // contract
    pub chain_admin_addr: Address,
    pub access_control_restriction: Address,
}
impl ZkStackConfig for GatewayChainUpgradeOutput {}
