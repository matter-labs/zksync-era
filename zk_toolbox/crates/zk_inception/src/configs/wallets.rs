use ethers::{core::rand::Rng, types::H256};
use serde::{Deserialize, Serialize};

use crate::{
    configs::{ReadConfig, SaveConfig},
    wallets::Wallet,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletsConfig {
    pub deployer: Option<Wallet>,
    pub operator: Wallet,
    pub blob_operator: Wallet,
    pub fee_account: Wallet,
    pub governor: Wallet,
}

impl WalletsConfig {
    /// Generate random wallets
    pub fn random(rng: &mut impl Rng) -> Self {
        Self {
            deployer: Some(Wallet::random(rng)),
            operator: Wallet::random(rng),
            blob_operator: Wallet::random(rng),
            fee_account: Wallet::random(rng),
            governor: Wallet::random(rng),
        }
    }

    /// Generate placeholder wallets
    pub fn empty() -> Self {
        Self {
            deployer: Some(Wallet::empty()),
            operator: Wallet::empty(),
            blob_operator: Wallet::empty(),
            fee_account: Wallet::empty(),
            governor: Wallet::empty(),
        }
    }
    pub fn deployer_private_key(&self) -> Option<H256> {
        self.deployer.as_ref().and_then(|wallet| wallet.private_key)
    }

    pub fn governor_private_key(&self) -> Option<H256> {
        self.governor.private_key
    }
}

impl ReadConfig for WalletsConfig {}
impl SaveConfig for WalletsConfig {}

/// ETH config from zkync repository
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct EthMnemonicConfig {
    pub(crate) test_mnemonic: String,
    pub(super) mnemonic: String,
    pub(crate) base_path: String,
}
