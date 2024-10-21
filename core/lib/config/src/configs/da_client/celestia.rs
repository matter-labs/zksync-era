use serde::Deserialize;
use zksync_basic_types::secrets::PrivateKey;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct CelestiaConfig {
    pub api_node_url: String,
    pub namespace: String,
    pub chain_id: String,
    pub timeout_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CelestiaSecrets {
    pub private_key: PrivateKey,
}
