use serde::{de::Error as DeError, Deserialize};
use serde_json::Value;
use smart_config::{
    de::{DeserializeContext, DeserializeParam, Optional},
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

/// Wallet configuration supporting both local private keys and GCP KMS keys.
///
/// Exactly one of `private_key` or `gcp_kms_resource` must be provided.
///
/// # Examples
///
/// ## Local private key (existing format)
/// ```yaml
/// operator:
///   private_key: "0x..."
/// ```
///
/// ## GCP KMS key (new format)
/// ```yaml
/// operator:
///   gcp_kms_resource: "projects/{project}/locations/{location}/keyRings/{ring}/cryptoKeys/{key}/cryptoKeyVersions/{version}"
/// ```
#[derive(Debug, Clone, DescribeConfig, DeserializeConfig)]
#[config(validate(
    Self::validate,
    "wallet configuration must have exactly one of `private_key` or `gcp_kms_resource`"
))]
pub struct Wallet {
    /// Address of the account. Used to validate private key integrity (for local keys).
    address: Option<Address>,
    /// Local private key for signing. Mutually exclusive with `gcp_kms_resource`.
    #[config(secret, with = Optional(K256PrivateKeyDeserializer))]
    private_key: Option<K256PrivateKey>,
    /// GCP KMS resource name for HSM-backed signing. Mutually exclusive with `private_key`.
    /// Format: `projects/{project}/locations/{location}/keyRings/{ring}/cryptoKeys/{key}/cryptoKeyVersions/{version}`
    #[config(secret)]
    gcp_kms_resource: Option<String>,
}

impl Wallet {
    fn validate(&self) -> Result<(), ErrorWithOrigin> {
        match (&self.private_key, &self.gcp_kms_resource) {
            (Some(pk), None) => {
                // Local key: validate address if provided.
                if let Some(address) = self.address {
                    if address != pk.address() {
                        return Err(ErrorWithOrigin::custom(
                            "Malformed wallet; `address` doesn't correspond to `private_key`",
                        ));
                    }
                }
                Ok(())
            }
            (None, Some(_)) => {
                // GCP KMS: address is required since it can only be fetched async from KMS
                // and many call sites need it synchronously.
                if self.address.is_none() {
                    return Err(ErrorWithOrigin::custom(
                        "GCP KMS wallet must have `address` configured",
                    ));
                }
                // Resource name format is validated by `parse_kms_resource_name`
                // in `operator_signer` at signer creation time.
                Ok(())
            }
            (Some(_), Some(_)) => Err(ErrorWithOrigin::custom(
                "Both `private_key` and `gcp_kms_resource` are set; only one should be provided",
            )),
            (None, None) => Err(ErrorWithOrigin::custom(
                "Neither `private_key` nor `gcp_kms_resource` is set; one must be provided",
            )),
        }
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
            private_key: Some(private_key),
            gcp_kms_resource: None,
        })
    }

    /// Returns the Ethereum address for this wallet.
    ///
    /// For local key wallets, derives from the private key if not explicitly set.
    /// For GCP KMS wallets, returns the configured address (required at validation time).
    pub fn address(&self) -> Address {
        if let Some(ref pk) = self.private_key {
            self.address.unwrap_or_else(|| pk.address())
        } else {
            // Safe: validate() ensures address is set for GCP KMS wallets.
            self.address
                .expect("GCP KMS wallet without address passed validation")
        }
    }

    /// Returns the local private key, if this is a local-key wallet.
    pub fn private_key(&self) -> &K256PrivateKey {
        self.private_key
            .as_ref()
            .expect("private_key() called on a GCP KMS wallet; use is_gcp_kms() to check first")
    }

    /// Returns the GCP KMS resource name, if this is a KMS wallet.
    pub fn gcp_kms_resource(&self) -> Option<&str> {
        self.gcp_kms_resource.as_deref()
    }

    /// Returns true if this wallet uses GCP KMS for signing.
    pub fn is_gcp_kms(&self) -> bool {
        self.gcp_kms_resource.is_some()
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
              gcp_kms_resource: ~
            blob_operator:
              address: 0x5927c313861c01b82a026e35d93cc787e5356c0f
              private_key: 0xc9ee945b2f6d4c462a743f5af3904a4ee78aec0218f1f4f3c53d0bfbf809b520
              gcp_kms_resource: ~
            fee_account:
              address: 0x7ea53e0f1eb0b3b578aeda336b2c3a778e04eebf
              private_key: 0xe338cadae0f665139a7a4f2b846b91e188a2d100dcd34f58771c903cd2b08cd1
            token_multiplier_setter:
              address: 0x1900678c093afec2558642bc4cae038254b9e664
              private_key: 0x2137749ca460802189d3eeb9be411128c28ce67edf0d2fd750212f96a888cfa5
              gcp_kms_resource: ~
            eth_proof_manager:
              address: 0x1900678c093afec2558642bc4cae038254b9e664
              private_key: 0x2137749ca460802189d3eeb9be411128c28ce67edf0d2fd750212f96a888cfa5
              gcp_kms_resource: ~
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
    fn parsing_gcp_kms_wallet() {
        let yaml = r#"
            operator:
              address: 0xabcf96e1ee478481042a0c4e34cdceceae01b154
              private_key: ~
              gcp_kms_resource: "projects/my-project/locations/us-central1/keyRings/my-ring/cryptoKeys/my-key/cryptoKeyVersions/1"
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let wallets: Wallets = test_complete(yaml).unwrap();
        let operator = wallets.operator.unwrap();
        assert!(operator.is_gcp_kms());
        assert_eq!(
            operator.gcp_kms_resource().unwrap(),
            "projects/my-project/locations/us-central1/keyRings/my-ring/cryptoKeys/my-key/cryptoKeyVersions/1"
        );
        assert_eq!(
            operator.address(),
            "0xabcf96e1ee478481042a0c4e34cdceceae01b154"
                .parse()
                .unwrap()
        );
    }

    #[test]
    fn parsing_error_gcp_kms_without_address() {
        let yaml = r#"
            operator:
              address: ~
              private_key: ~
              gcp_kms_resource: "projects/p/locations/l/keyRings/r/cryptoKeys/k/cryptoKeyVersions/1"
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let err = test_complete::<Wallets>(yaml).unwrap_err();
        assert_eq!(err.len(), 1, "{err}");
        let err = err.first().inner().to_string();
        assert!(err.contains("address"), "{err}");
    }

    #[test]
    fn parsing_error() {
        let yaml = r#"
            operator:
              address: 0xabcf96e1ee478481042a0c4e34cdceceae01b154
              private_key: 0xf00bf4165f9e1a67841b981949033c06c1423dab34c33d6d1237ae14d85bd728
              gcp_kms_resource: ~
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let err = test_complete::<Wallets>(yaml).unwrap_err();
        assert_eq!(err.len(), 1, "{err}");
        let err = err.first().inner().to_string();
        assert!(err.contains("Malformed wallet"), "{err}");
    }

    #[test]
    fn parsing_error_both_private_key_and_gcp() {
        let yaml = r#"
            operator:
              address: ~
              private_key: 0xf00bf4165f9e1a67841b981949033c06c1423dab34c33d6d1237ae14d85bd729
              gcp_kms_resource: "projects/p/locations/l/keyRings/r/cryptoKeys/k/cryptoKeyVersions/1"
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let err = test_complete::<Wallets>(yaml).unwrap_err();
        assert_eq!(err.len(), 1, "{err}");
        let err = err.first().inner().to_string();
        assert!(err.contains("only one should be provided"), "{err}");
    }
}
