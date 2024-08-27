use serde::Deserialize;

use crate::configs::deserialize_stringified_any;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AvailConfig {
    pub api_node_url: String,
    pub bridge_api_url: String,
    pub seed: String,
    #[serde(deserialize_with = "deserialize_stringified_any")]
    pub app_id: u32,
    #[serde(deserialize_with = "deserialize_stringified_any")]
    pub timeout: usize,
    #[serde(deserialize_with = "deserialize_stringified_any")]
    pub max_retries: usize,
}
