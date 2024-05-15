use ethers::{
    core::rand::Rng,
    signers::{LocalWallet, Signer},
    types::{H160, H256},
};
use serde::{Deserialize, Serialize};

/// Wallet struct where we store the address and private key of the wallet
/// The private key is optional, as we might not have it if
/// user want to provide only the address that will be used during initialization
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub address: H160,
    pub private_key: Option<H256>,
}

impl Wallet {
    /// Generate a random wallet
    pub fn random(rng: &mut impl Rng) -> Self {
        let private_key = H256(rng.gen());
        let local_wallet = LocalWallet::from_bytes(private_key.as_bytes()).unwrap();

        Self {
            address: local_wallet.address(),
            private_key: Some(private_key),
        }
    }

    /// Generate an empty wallet, could be usefull for generating placeholder wallets
    pub fn empty() -> Self {
        Self {
            address: H160::zero(),
            private_key: Some(H256::zero()),
        }
    }
}
