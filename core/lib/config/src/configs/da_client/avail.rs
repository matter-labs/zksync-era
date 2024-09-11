use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AvailConfig {
    pub api_node_url: String,
    pub bridge_api_url: String,
    pub seed: String,
    pub app_id: u32,
    pub timeout: usize,
    pub max_retries: usize,
}
