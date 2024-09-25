use serde::Deserialize;
use zksync_basic_types::secrets::SeedPhrase;

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct AvailConfig {
    pub api_node_url: String,
    pub bridge_api_url: String,
    pub app_id: u32,
    pub timeout: usize,
    pub max_retries: usize,
    #[serde(skip)]
    pub secrets: Option<AvailSecrets>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AvailSecrets {
    pub seed_phrase: SeedPhrase,
}
