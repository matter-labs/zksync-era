use serde::Deserialize;
use zksync_basic_types::secrets::PrivateKey;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct EigenConfig {
    pub rpc_node_url: String,
    pub inclusion_polling_interval_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EigenSecrets {
    pub private_key: PrivateKey,
}
