use std::{collections::HashMap, str::FromStr};

use ethers::{
    prelude::U256,
    types::{Address, H256},
};
use rand::Rng;
use serde::{Deserialize, Serialize};
use types::L1BatchCommitmentMode;
use zksync_basic_types::L2ChainId;
use zksync_protobuf_config::proto::genesis::L1BatchCommitDataGeneratorMode;

use crate::{
    apply_l1_to_l2_alias,
    consts::INITIAL_DEPLOYMENT_FILE,
    forge_interface::deploy_ecosystem::input::InitialDeploymentConfig,
    traits::{FileConfigWithDefaultName, ZkToolboxConfig},
    ChainConfig, ContractsConfig, GenesisConfig, WalletsConfig, ERC20_DEPLOYMENT_FILE,
};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GatewayChainUpgradeInput {
    // This should be the address that controls the current `ChainAdmin`
    // contract
    pub owner_address: Address,
    pub chain: GatewayChainUpgradeChain,
}
impl ZkToolboxConfig for GatewayChainUpgradeInput {}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GatewayChainUpgradeChain {
    pub chain_id: L2ChainId,
    pub diamond_proxy_address: Address,
    pub validium_mode: bool,
    pub permanent_rollup: bool,
}

impl GatewayChainUpgradeInput {
    pub fn new(current_chain_config: &ChainConfig) -> Self {
        let contracts_config = current_chain_config.get_contracts_config().unwrap();

        let validum = current_chain_config
            .get_genesis_config()
            .unwrap()
            .l1_batch_commit_data_generator_mode
            == L1BatchCommitmentMode::Validium;

        Self {
            owner_address: current_chain_config
                .get_wallets_config()
                .unwrap()
                .governor
                .address,
            chain: GatewayChainUpgradeChain {
                chain_id: current_chain_config.chain_id,
                diamond_proxy_address: contracts_config.l1.diamond_proxy_addr,
                validium_mode: validum,
                // FIXME: we assume that all rollup chains want to forever remain this way
                permanent_rollup: !validum,
            },
        }
    }
}
