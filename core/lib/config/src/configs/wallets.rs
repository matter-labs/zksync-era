use serde::{de::Error as DeError, Deserialize};
use smart_config::{
    de::{Custom, DeserializeContext},
    metadata::ParamMetadata,
    DescribeConfig, DeserializeConfig, ErrorWithOrigin,
};
use zksync_basic_types::{Address, H160, H256};
use zksync_crypto_primitives::K256PrivateKey;

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub struct AddressWallet {
    /// Address of the account.
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

fn deserialize_key(
    ctx: DeserializeContext<'_>,
    param: &'static ParamMetadata,
) -> Result<K256PrivateKey, ErrorWithOrigin> {
    let de = ctx.current_value_deserializer(param.name)?;
    let key = H256::deserialize(de)?;
    K256PrivateKey::from_bytes(key).map_err(DeError::custom)
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
pub struct Wallet {
    /// Address of the account. Used to validate private key integrity.
    address: Option<Address>,
    #[config(secret, with = Custom![_;  str](deserialize_key))]
    private_key: K256PrivateKey,
}

impl Wallet {
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
            address,
            private_key,
        })
    }

    pub fn address(&self) -> Address {
        self.address.unwrap_or_else(|| self.private_key.address())
    }

    pub fn private_key(&self) -> &K256PrivateKey {
        &self.private_key
    }
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct Wallets {
    /// Wallet for the SL operator.
    #[config(nest)]
    pub operator: Option<Wallet>,
    /// Wallet for the SL operator when using blob commitments.
    #[config(nest)]
    pub blob_operator: Option<Wallet>,
    /// Fee account.
    #[config(nest)]
    pub fee_account: Option<AddressWallet>,
    #[config(nest)]
    pub token_multiplier_setter: Option<Wallet>,
}

impl Wallets {
    pub fn for_tests() -> Wallets {
        Wallets {
            operator: Some(Wallet::from_private_key_bytes(H256::repeat_byte(0x1), None).unwrap()),
            blob_operator: Some(
                Wallet::from_private_key_bytes(H256::repeat_byte(0x2), None).unwrap(),
            ),
            fee_account: Some(AddressWallet::from_address(H160::repeat_byte(0x3))),
            token_multiplier_setter: Some(
                Wallet::from_private_key_bytes(H256::repeat_byte(0x4), None).unwrap(),
            ),
        }
    }
}
