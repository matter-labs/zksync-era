use serde::{de::Error as DeError, Deserialize};
use serde_json::Value;
use smart_config::{
    de::{DeserializeContext, DeserializeParam},
    metadata::{BasicTypes, ParamMetadata},
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

#[derive(Debug)]
struct K256PrivateKeyDeserializer;

impl DeserializeParam<K256PrivateKey> for K256PrivateKeyDeserializer {
    const EXPECTING: BasicTypes = BasicTypes::STRING;

    fn deserialize_param(
        &self,
        ctx: DeserializeContext<'_>,
        param: &'static ParamMetadata,
    ) -> Result<K256PrivateKey, ErrorWithOrigin> {
        let de = ctx.current_value_deserializer(param.name)?;
        let key = H256::deserialize(de)?;
        K256PrivateKey::from_bytes(key).map_err(DeError::custom)
    }

    fn serialize_param(&self, param: &K256PrivateKey) -> Value {
        let key_bytes = *param.expose_secret().as_ref();
        serde_json::to_value(H256(key_bytes)).unwrap()
    }
}

#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(validate(Self::validate_address, "`address` should correspond to `private_key`"))]
pub struct Wallet {
    /// Address of the account. Used to validate private key integrity.
    address: Option<Address>,
    #[config(secret, with = K256PrivateKeyDeserializer)]
    private_key: K256PrivateKey,
}

impl Wallet {
    fn validate_address(&self) -> Result<(), ErrorWithOrigin> {
        if let Some(address) = self.address {
            if address != self.private_key.address() {
                return Err(ErrorWithOrigin::custom(
                    "Malformed wallet; `address` doesn't correspond to `private_key`",
                ));
            }
        }
        Ok(())
    }

    fn from_private_key_bytes(
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
    #[config(nest)]
    pub eth_proof_manager: Option<Wallet>,
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
            eth_proof_manager: Some(
                Wallet::from_private_key_bytes(H256::repeat_byte(0x5), None).unwrap(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Yaml};

    use super::*;

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
            operator:
              address: 0xabcf96e1ee478481042a0c4e34cdceceae01b154
              private_key: 0xf00bf4165f9e1a67841b981949033c06c1423dab34c33d6d1237ae14d85bd729
            blob_operator:
              address: 0x5927c313861c01b82a026e35d93cc787e5356c0f
              private_key: 0xc9ee945b2f6d4c462a743f5af3904a4ee78aec0218f1f4f3c53d0bfbf809b520
            fee_account:
              address: 0x7ea53e0f1eb0b3b578aeda336b2c3a778e04eebf
              private_key: 0xe338cadae0f665139a7a4f2b846b91e188a2d100dcd34f58771c903cd2b08cd1
            token_multiplier_setter:
              address: 0x1900678c093afec2558642bc4cae038254b9e664
              private_key: 0x2137749ca460802189d3eeb9be411128c28ce67edf0d2fd750212f96a888cfa5
            eth_proof_manager:
              address: 0x1900678c093afec2558642bc4cae038254b9e664
              private_key: 0x2137749ca460802189d3eeb9be411128c28ce67edf0d2fd750212f96a888cfa5
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let wallets: Wallets = test_complete(yaml).unwrap();
        assert_eq!(
            wallets.operator.unwrap().address(),
            "0xabcf96e1ee478481042a0c4e34cdceceae01b154"
                .parse()
                .unwrap()
        );
        assert_eq!(
            wallets.blob_operator.unwrap().address(),
            "0x5927c313861c01b82a026e35d93cc787e5356c0f"
                .parse()
                .unwrap()
        );
        assert_eq!(
            wallets.fee_account.unwrap().address(),
            "0x7ea53e0f1eb0b3b578aeda336b2c3a778e04eebf"
                .parse()
                .unwrap()
        );
        assert_eq!(
            wallets.token_multiplier_setter.unwrap().address(),
            "0x1900678c093afec2558642bc4cae038254b9e664"
                .parse()
                .unwrap()
        );
        assert_eq!(
            wallets.eth_proof_manager.unwrap().address(),
            "0x1900678c093afec2558642bc4cae038254b9e664"
                .parse()
                .unwrap()
        );
    }

    #[test]
    fn parsing_error() {
        let yaml = r#"
            operator:
              address: 0xabcf96e1ee478481042a0c4e34cdceceae01b154
              private_key: 0xf00bf4165f9e1a67841b981949033c06c1423dab34c33d6d1237ae14d85bd728
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let err = test_complete::<Wallets>(yaml).unwrap_err();
        assert_eq!(err.len(), 1, "{err}");
        let err = err.first().inner().to_string();
        assert!(err.contains("Malformed wallet"), "{err}");
    }
}
