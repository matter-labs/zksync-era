use ethers::types::Address;
use serde::{Deserialize, Serialize};
use zkstack_cli_types::L1BatchCommitmentMode;
use zksync_basic_types::L2ChainId;

use crate::{traits::ZkStackConfig, ChainConfig};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct GatewayChainUpgradeInput {
    // This should be the address that controls the current `ChainAdmin`
    // contract
    pub owner_address: Address,
    pub chain: GatewayChainUpgradeChain,
}
impl ZkStackConfig for GatewayChainUpgradeInput {}

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
                // TODO(EVM-860): we assume that all rollup chains want to forever remain this way
                permanent_rollup: !validum,
            },
        }
    }
}
