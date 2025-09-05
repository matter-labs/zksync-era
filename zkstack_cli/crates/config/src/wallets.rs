use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use zkstack_cli_common::wallets::Wallet;

use crate::{
    consts::WALLETS_FILE,
    traits::{FileConfigWithDefaultName, ZkStackConfigTrait},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletsConfig {
    pub deployer: Option<Wallet>,
    pub operator: Wallet,
    pub blob_operator: Wallet,
    pub fee_account: Wallet,
    pub governor: Wallet,
    pub token_multiplier_setter: Option<Wallet>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub test_wallet: Option<Wallet>,
}

impl WalletsConfig {
    /// Generate random wallets
    pub fn random(rng: &mut (impl CryptoRng + Rng)) -> Self {
        Self {
            deployer: Some(Wallet::random(rng)),
            operator: Wallet::random(rng),
            blob_operator: Wallet::random(rng),
            fee_account: Wallet::random(rng),
            governor: Wallet::random(rng),
            token_multiplier_setter: Some(Wallet::random(rng)),
            test_wallet: None,
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
            token_multiplier_setter: Some(Wallet::empty()),
            test_wallet: None,
        }
    }
}

impl FileConfigWithDefaultName for WalletsConfig {
    const FILE_NAME: &'static str = WALLETS_FILE;
}

/// ETH config from zkync repository
#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct EthMnemonicConfig {
    pub(crate) test_mnemonic: String,
    pub(super) mnemonic: String,
    pub(crate) base_path: String,
}

impl ZkStackConfigTrait for EthMnemonicConfig {}

impl ZkStackConfigTrait for WalletsConfig {}
