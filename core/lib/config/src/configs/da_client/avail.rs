use serde::Deserialize;
use zksync_basic_types::api_key::APIKey;
use zksync_basic_types::seed_phrase::SeedPhrase;

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub enum AvailClientConfig {
    Default(AvailDefaultConfig),
    GasRelay(AvailGasRelayConfig),
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AvailConfig {
    pub bridge_api_url: String,
    pub timeout: usize,
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
}

#[derive(Clone, Debug, PartialEq)]
pub struct AvailSecrets {
    pub seed_phrase: Option<SeedPhrase>,
    pub gas_relay_api_key: Option<APIKey>,
}
