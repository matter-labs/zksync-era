use ethers::{
    core::rand::{CryptoRng, Rng},
    signers::{coins_bip39::English, LocalWallet, MnemonicBuilder, Signer},
    types::{Address, H256},
};
use serde::{Deserialize, Serialize};
use zkstack_cli_types::parse_h256;

#[derive(Serialize, Deserialize)]
struct WalletSerde {
    pub address: Address,
    pub private_key: Option<H256>,
}

#[derive(Debug, Clone)]
pub struct Wallet {
    pub address: Address,
    pub private_key: Option<LocalWallet>,
}

impl<'de> Deserialize<'de> for Wallet {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let x = WalletSerde::deserialize(d)?;
        Ok(match x.private_key {
            None => Self {
                address: x.address,
                private_key: None,
            },
            Some(k) => {
                let k = LocalWallet::from_bytes(k.as_bytes()).map_err(serde::de::Error::custom)?;
                if k.address() != x.address {
                    return Err(serde::de::Error::custom(format!(
                        "address does not match private key: got address {:#x}, want {:#x}",
                        x.address,
                        k.address(),
                    )));
                }
                Self::new(k)
            }
        })
    }
}

impl Serialize for Wallet {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        WalletSerde {
            address: self.address,
            private_key: self.private_key_h256(),
        }
        .serialize(s)
    }
}

impl Wallet {
    pub fn private_key_h256(&self) -> Option<H256> {
        self.private_key
            .as_ref()
            .map(|k| parse_h256(k.signer().to_bytes().as_slice()).unwrap())
    }

    pub fn random(rng: &mut (impl Rng + CryptoRng)) -> Self {
        Self::new(LocalWallet::new(rng))
    }

    pub fn new(private_key: LocalWallet) -> Self {
        Self {
            address: private_key.address(),
            private_key: Some(private_key),
        }
    }

    pub fn from_mnemonic(mnemonic: &str, base_path: &str, index: u32) -> anyhow::Result<Self> {
        let wallet = MnemonicBuilder::<English>::default()
            .phrase(mnemonic)
            .derivation_path(&format!("{}/{}", base_path, index))?
            .build()?;
        Ok(Self::new(wallet))
    }

    pub fn empty() -> Self {
        Self {
            address: Address::zero(),
            private_key: None,
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
