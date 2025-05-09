//! Mirrored types for data availability configs.

use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AvailDefaultConfig {
    pub api_node_url: String,
    pub app_id: u32,
    pub finality_state: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AvailGasRelayConfig {
    pub gas_relay_api_url: String,
    pub max_retries: usize,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "avail_client_type")]
pub enum AvailClientConfig {
    FullClient(AvailDefaultConfig),
    GasRelay(AvailGasRelayConfig),
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AvailConfig {
    pub bridge_api_url: String,
    pub timeout_ms: usize,
    #[serde(flatten)]
    pub config: AvailClientConfig,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct AvailSecrets {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_phrase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gas_relay_api_key: Option<String>,
}
