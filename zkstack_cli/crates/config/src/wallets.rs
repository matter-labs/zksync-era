use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};
use zkstack_cli_common::wallets::Wallet;

use crate::{
    consts::WALLETS_FILE,
    traits::{FileConfigTrait, FileConfigWithDefaultName},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletsConfig {
    pub deployer: Option<Wallet>,
    /// This wallet can be used for any operations (commit, prove, execute, etc.)
    pub operator: Wallet,
    /// This wallet should be used only for 'commit' when using blobs.
    pub blob_operator: Wallet,
    /// These are optional wallets, that can be used for prove & execute (when these are handled by different entities).
    pub prove_operator: Option<Wallet>,
    pub execute_operator: Option<Wallet>,

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
            prove_operator: Some(Wallet::random(rng)),
            execute_operator: Some(Wallet::random(rng)),
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
            prove_operator: None,
            execute_operator: None,
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

impl FileConfigTrait for EthMnemonicConfig {}

impl FileConfigTrait for WalletsConfig {}
