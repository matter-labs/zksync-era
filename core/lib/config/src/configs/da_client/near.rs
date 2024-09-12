use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct NearConfig {
    pub evm_provider_url: String,
    pub rpc_client_url: String,
    pub blob_contract: String,
    pub bridge_contract: String,
    pub account_id: String,
    pub secret_key: String,
}
