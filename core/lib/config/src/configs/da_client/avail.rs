use serde::Deserialize;
use zksync_basic_types::secrets::{APIKey, SeedPhrase};

pub const AVAIL_GAS_RELAY_CLIENT_NAME: &str = "GasRelay";
pub const AVAIL_FULL_CLIENT_NAME: &str = "FullClient";

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(tag = "avail_client")]
pub enum AvailClientConfig {
    FullClient(AvailDefaultConfig),
    GasRelay(AvailGasRelayConfig),
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AvailConfig {
    pub bridge_api_url: String,
    pub timeout_ms: usize,
    #[serde(flatten)]
    pub config: AvailClientConfig,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AvailDefaultConfig {
    pub api_node_url: String,
    pub app_id: u32,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AvailGasRelayConfig {
    pub gas_relay_api_url: String,
    pub max_retries: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AvailSecrets {
    pub seed_phrase: Option<SeedPhrase>,
    pub gas_relay_api_key: Option<APIKey>,
}
