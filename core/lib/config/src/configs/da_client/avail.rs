use std::time::Duration;

use serde::{Deserialize, Serialize};
use zksync_basic_types::secrets::{APIKey, SeedPhrase};

pub const AVAIL_GAS_RELAY_CLIENT_NAME: &str = "GasRelay";
pub const AVAIL_FULL_CLIENT_NAME: &str = "FullClient";
pub const DEFAULT_DISPATCH_TIMEOUT_MS: u64 = 180_000; // 3 minutes

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "avail_client")]
pub enum AvailClientConfig {
    FullClient(AvailDefaultConfig),
    GasRelay(AvailGasRelayConfig),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvailConfig {
    pub bridge_api_url: String,
    pub timeout_ms: usize,
    #[serde(flatten)]
    pub config: AvailClientConfig,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvailDefaultConfig {
    pub api_node_url: String,
    pub app_id: u32,
    pub finality_state: Option<String>,
    pub dispatch_timeout_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AvailGasRelayConfig {
    pub gas_relay_api_url: String,
    pub max_retries: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AvailSecrets {
    pub seed_phrase: Option<SeedPhrase>,
    pub gas_relay_api_key: Option<APIKey>,
}

impl AvailDefaultConfig {
    pub fn dispatch_timeout(&self) -> Duration {
        Duration::from_millis(
            self.dispatch_timeout_ms
                .unwrap_or(DEFAULT_DISPATCH_TIMEOUT_MS),
        )
    }
}
