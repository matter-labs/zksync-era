use serde::Deserialize;
use zksync_basic_types::secrets::PrivateKey;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct NearConfig {
    pub evm_provider_url: String,
    pub rpc_client_url: String,
    pub blob_contract: String,
    pub bridge_contract: String,
    pub account_id: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct NearSecrets {
    pub secret_key: PrivateKey,
}
