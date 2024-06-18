use alloy::{
    primitives::{Address, B256},
    signers::local::{coins_bip39::English, MnemonicBuilder},
};
use rand::{CryptoRng, Rng};
use serde::{Deserialize, Serialize};

use crate::ethereum::get_address_from_private_key;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Wallet {
    pub address: Address,
    pub private_key: Option<B256>,
}

impl Wallet {
    pub fn random<R>(rng: &mut R) -> Self
    where
        R: Rng + CryptoRng,
    {
        let private_key = B256::random_with(rng);
        let address = get_address_from_private_key(&private_key);

        Self {
            address,
            private_key: Some(private_key),
        }
    }

    pub fn new_with_key(private_key: B256) -> Self {
        Self {
            address: get_address_from_private_key(&private_key),
            private_key: Some(private_key),
        }
    }

    pub fn from_mnemonic(mnemonic: &str, base_path: &str, index: u32) -> anyhow::Result<Self> {
        let wallet = MnemonicBuilder::<English>::default()
            .phrase(mnemonic)
            .derivation_path(format!("{}/{}", base_path, index))?
            .build()?;
        let private_key = wallet.to_bytes();
        Ok(Self::new_with_key(private_key))
    }

    pub fn empty() -> Self {
        Self {
            address: Address::ZERO,
            private_key: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

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
            Address::from_str("0xa61464658AfeAf65CccaaFD3a512b69A83B77618")
                .expect("Invalid address")
        );
    }
}
