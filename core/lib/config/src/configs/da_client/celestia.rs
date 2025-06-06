use std::time::Duration;

use smart_config::{de::FromSecretString, DescribeConfig, DeserializeConfig};
use zksync_basic_types::secrets::PrivateKey;

#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct CelestiaConfig {
    pub api_node_url: String,
    pub namespace: String,
    pub chain_id: String,
    #[config(default_t = Duration::from_secs(30))]
    pub timeout: Duration,
}

#[derive(Clone, Debug, DescribeConfig, DeserializeConfig)]
pub struct CelestiaSecrets {
    #[config(with = FromSecretString)]
    pub private_key: PrivateKey,
}
