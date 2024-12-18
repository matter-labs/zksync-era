use serde::Deserialize;
use smart_config::{de::FromSecretString, DescribeConfig, DeserializeConfig};
use zksync_basic_types::secrets::PrivateKey;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct EigenConfig {
    pub rpc_node_url: String,
    pub inclusion_polling_interval_ms: u64,
}

#[derive(Clone, Debug, DescribeConfig, DeserializeConfig)]
pub struct EigenSecrets {
    #[config(with = FromSecretString)]
    pub private_key: PrivateKey,
}
