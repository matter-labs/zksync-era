use serde::Deserialize;
use zksync_basic_types::secrets::AuthToken;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct CelestiaConfig {
    pub api_node_url: String,
    pub namespace: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CelestiaSecrets {
    pub auth_token: AuthToken,
}
