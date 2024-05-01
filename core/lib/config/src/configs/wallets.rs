use zksync_basic_types::{Address, H160, H256};
use zksync_crypto_primitives::K256PrivateKey;

#[derive(Debug, Clone)]
pub struct AddressWallet {
    address: Address,
}

impl AddressWallet {
    pub fn from_address(address: Address) -> Self {
        Self { address }
    }

    pub fn address(&self) -> Address {
        self.address
    }
}

#[derive(Debug, Clone)]
pub struct Wallet {
    address: Address,
    private_key: K256PrivateKey,
}

impl Wallet {
    pub fn new(private_key: K256PrivateKey) -> Self {
        Self {
            address: private_key.address(),
            private_key,
        }
    }

    pub fn from_private_key_bytes(
        private_key_bytes: H256,
        address: Option<Address>,
    ) -> anyhow::Result<Self> {
        let private_key = K256PrivateKey::from_bytes(private_key_bytes)?;
        let calculated_address = private_key.address();
        if let Some(address) = address {
            anyhow::ensure!(
                calculated_address == address,
                "Malformed wallet, address doesn't correspond private_key"
            );
        }

        Ok(Self {
            address: calculated_address,
            private_key,
        })
    }

    pub fn address(&self) -> Address {
        self.address
    }

    pub fn private_key(&self) -> &K256PrivateKey {
        &self.private_key
    }
}

#[derive(Debug, Clone)]
pub struct EthSender {
    pub operator: Wallet,
    pub blob_operator: Option<Wallet>,
}

#[derive(Debug, Clone)]
pub struct StateKeeper {
    pub fee_account: AddressWallet,
}

#[derive(Debug, Clone)]
pub struct Wallets {
    pub eth_sender: Option<EthSender>,
    pub state_keeper: Option<StateKeeper>,
}

impl Wallets {
    pub fn for_tests() -> Wallets {
        Wallets {
            eth_sender: Some(EthSender {
                operator: Wallet::from_private_key_bytes(H256::repeat_byte(0x1), None).unwrap(),
                blob_operator: Some(
                    Wallet::from_private_key_bytes(H256::repeat_byte(0x2), None).unwrap(),
                ),
            }),
            state_keeper: Some(StateKeeper {
                fee_account: AddressWallet::from_address(H160::repeat_byte(0x3)),
            }),
        }
    }
}
