use zksync_basic_types::{Address, H256};

pub struct Wallet {
    address: Address,
    private_key: Option<H256>,
}

impl Wallet {
    pub fn from_address(address: Address) -> Self {
        Self {
            address,
            private_key: None,
        }
    }

    pub fn from_private_key(private_key: H256) -> Self {
        // let address = PackedEthSignature::address_from_private_key(&private_key)
        //     .expect("Failed to get address from private key");
        // TODO fix it
        let address = Address::zero();
        Self {
            address,
            private_key: Some(private_key),
        }
    }

    pub fn address(&self) -> Address {
        self.address
    }
    pub fn private_key(&self) -> Option<H256> {
        self.private_key
    }
}

pub struct EthSender {
    pub operator: Wallet,
    pub blob_operator: Wallet,
}

pub struct StateKeeper {
    pub fee_account: Wallet,
}

pub struct Wallets {
    pub eth_sender: Option<EthSender>,
    pub state_keeper: Option<StateKeeper>,
}
