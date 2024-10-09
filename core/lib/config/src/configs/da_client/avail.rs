use serde::Deserialize;
use zksync_basic_types::seed_phrase::SeedPhrase;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AvailConfig {
    pub api_node_url: Option<String>,
    pub bridge_api_url: String,
    pub app_id: Option<u32>,
    pub timeout: usize,
    pub max_retries: usize,
    pub gas_relay_mode: bool,
    pub gas_relay_api_url: Option<String>,
    pub gas_relay_api_key: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AvailSecrets {
    pub seed_phrase: Option<SeedPhrase>,
}
