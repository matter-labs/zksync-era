use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct NearConfig {
    pub evm_provider_url: String,
    pub da_rpc_url: String,
    pub contract: String,
    pub bridge_contract: String,
    pub account_id: String,
    pub secret_key: String,
}
