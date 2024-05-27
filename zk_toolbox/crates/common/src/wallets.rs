use alloy_primitives::{Address, B256};
use ethers::{
    core::rand::Rng,
    signers::{coins_bip39::English, LocalWallet, MnemonicBuilder, Signer},
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub address: Address,
    pub private_key: Option<B256>,
}

impl Wallet {
    pub fn random(rng: &mut impl Rng) -> Self {
        let private_key = B256::random_with(rng);
        let local_wallet = LocalWallet::from_bytes(private_key.as_slice()).unwrap();

        Self {
            address: Address::from_slice(local_wallet.address().as_bytes()),
            private_key: Some(private_key),
        }
    }

    pub fn new_with_key(private_key: B256) -> Self {
        let local_wallet = LocalWallet::from_bytes(private_key.as_slice()).unwrap();
        Self {
            address: Address::from_slice(local_wallet.address().as_bytes()),
            private_key: Some(private_key),
        }
    }

    pub fn from_mnemonic(mnemonic: &str, base_path: &str, index: u32) -> anyhow::Result<Self> {
        let wallet = MnemonicBuilder::<English>::default()
            .phrase(mnemonic)
            .derivation_path(&format!("{}/{}", base_path, index))?
            .build()?;
        let private_key = B256::from_slice(&wallet.signer().to_bytes());
        Ok(Self::new_with_key(private_key))
    }

    pub fn empty() -> Self {
        Self {
            address: Address::ZERO,
            private_key: Some(B256::ZERO),
        }
    }
}

#[test]
fn test_load_localhost_wallets() {
    let wallet = Wallet::from_mnemonic(
        "stuff slice staff easily soup parent arm payment cotton trade scatter struggle",
        "m/44'/60'/0'/0",
        1,
    )
    .unwrap();
    assert_eq!(
        wallet.address,
        Address::from_slice(
            &ethers::utils::hex::decode("0xa61464658AfeAf65CccaaFD3a512b69A83B77618").unwrap()
        )
    );
}
