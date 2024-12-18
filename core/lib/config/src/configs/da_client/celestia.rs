use serde::Deserialize;
use smart_config::{de::FromSecretString, DescribeConfig, DeserializeConfig};
use zksync_basic_types::secrets::PrivateKey;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct CelestiaConfig {
    pub api_node_url: String,
    pub namespace: String,
    pub chain_id: String,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, DescribeConfig, DeserializeConfig)]
pub struct CelestiaSecrets {
    #[config(with = FromSecretString)]
    pub private_key: PrivateKey,
}
