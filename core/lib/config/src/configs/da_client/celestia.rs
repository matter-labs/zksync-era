use std::time::Duration;

use serde::Deserialize;
use smart_config::{de::FromSecretString, DescribeConfig, DeserializeConfig};
use zksync_basic_types::secrets::PrivateKey;

// TODO: remove `#[derive(Deserialize)]` once env-based config in EN is reworked

#[derive(Clone, Debug, PartialEq, Deserialize, DescribeConfig, DeserializeConfig)]
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
