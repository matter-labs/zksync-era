use std::time::Duration;

use smart_config::{de::FromSecretString, DescribeConfig, DeserializeConfig};
use zksync_basic_types::secrets::PrivateKey;

#[derive(Clone, Debug, DescribeConfig, DeserializeConfig)]
pub struct CelestiaConfig {
    pub api_node_url: String,
    #[config(with = FromSecretString, deprecated = ".secrets.private_key")]
    pub private_key: PrivateKey,
    pub namespace: String,
    pub chain_id: String,
    #[config(default_t = Duration::from_secs(30))]
    pub timeout: Duration,
}
