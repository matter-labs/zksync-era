use serde::Deserialize;
use zksync_basic_types::secrets::PrivateKey;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct CelestiaConfig {
    pub api_node_url: String,
    pub namespace: String,
    #[serde(skip)]
    pub secrets: Option<CelestiaSecrets>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CelestiaSecrets {
    pub private_key: PrivateKey,
}
