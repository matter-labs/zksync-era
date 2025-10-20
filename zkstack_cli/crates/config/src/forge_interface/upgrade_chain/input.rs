use anyhow::Context;
use ethers::types::Address;
use serde::{Deserialize, Serialize};
use zkstack_cli_types::L1BatchCommitmentMode;
use zksync_basic_types::L2ChainId;

use crate::{traits::FileConfigTrait, ChainConfig};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChainUpgradeInput {
    // This should be the address that controls the current `ChainAdmin`
    // contract
    pub owner_address: Address,
    pub chain: ChainUpgradeChain,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct ChainUpgradeChain {
    pub chain_id: L2ChainId,
    pub diamond_proxy_address: Address,
    pub validium_mode: bool,
    pub permanent_rollup: bool,
}

impl ChainUpgradeInput {
    pub async fn new(current_chain_config: &ChainConfig) -> anyhow::Result<Self> {
        let contracts_config = current_chain_config
            .get_contracts_config()
            .context("failed loading contracts config")?;

        let validum = current_chain_config
            .get_genesis_config()
            .await
            .context("failed loading genesis config")?
            .l1_batch_commitment_mode()?
            == L1BatchCommitmentMode::Validium;

        Ok(Self {
            owner_address: current_chain_config
                .get_wallets_config()
                .context("failed loading wallets config")?
                .governor
                .address,
            chain: ChainUpgradeChain {
                chain_id: current_chain_config.chain_id,
                diamond_proxy_address: contracts_config.l1.diamond_proxy_addr,
                validium_mode: validum,
                // TODO(EVM-860): we assume that all rollup chains want to forever remain this way
                permanent_rollup: !validum,
            },
        })
    }
}

impl FileConfigTrait for ChainUpgradeInput {}
