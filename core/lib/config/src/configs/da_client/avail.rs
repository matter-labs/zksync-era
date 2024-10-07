use serde::Deserialize;
use zksync_basic_types::seed_phrase::SeedPhrase;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AvailConfig {
    pub api_node_url: String,
    pub bridge_api_url: String,
    pub app_id: u32,
    pub timeout: usize,
    pub max_retries: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AvailSecrets {
    pub seed_phrase: Option<SeedPhrase>,
}
