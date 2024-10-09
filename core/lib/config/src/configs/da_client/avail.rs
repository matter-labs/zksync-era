use std::str::FromStr;

use secrecy::ExposeSecret as _;
use secrecy::Secret;
use serde::Deserialize;
use zksync_basic_types::seed_phrase::SeedPhrase;

#[derive(Clone, Debug)]
pub struct GasRelayAPIKey(pub Secret<String>);

impl PartialEq for GasRelayAPIKey {
    fn eq(&self, other: &Self) -> bool {
        self.0.expose_secret().eq(other.0.expose_secret())
    }
}

impl FromStr for GasRelayAPIKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(GasRelayAPIKey(s.parse()?))
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct AvailConfig {
    pub api_node_url: Option<String>,
    pub bridge_api_url: String,
    pub app_id: Option<u32>,
    pub timeout: usize,
    pub max_retries: usize,
    pub gas_relay_mode: bool,
    pub gas_relay_api_url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AvailSecrets {
    pub seed_phrase: Option<SeedPhrase>,
    pub gas_relay_api_key: Option<GasRelayAPIKey>,
}
