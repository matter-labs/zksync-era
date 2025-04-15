use std::time::Duration;

use serde::Deserialize;
use smart_config::{
    de::{FromSecretString, WellKnown},
    DescribeConfig, DeserializeConfig, Serde,
};
use zksync_basic_types::secrets::{APIKey, SeedPhrase};

// FIXME: do URL params have reasonable defaults?

#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(tag = "avail_client_type")]
pub enum AvailClientConfig {
    FullClient(AvailDefaultConfig),
    GasRelay(AvailGasRelayConfig),
}

#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct AvailConfig {
    pub bridge_api_url: String,
    #[config(default_t = Duration::from_secs(30))]
    pub timeout: Duration,
    #[config(flatten)]
    pub config: AvailClientConfig,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Deserialize)]
pub enum AvailFinalityState {
    #[default]
    #[serde(rename = "inBlock")]
    InBlock,
    #[serde(rename = "finalized")]
    Finalized,
}

impl AvailFinalityState {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InBlock => "inBlock",
            Self::Finalized => "finalized",
        }
    }
}

impl WellKnown for AvailFinalityState {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct AvailDefaultConfig {
    pub api_node_url: String,
    pub app_id: u32,
    #[config(default)]
    pub finality_state: AvailFinalityState,
    #[config(default_t = Duration::from_secs(180))]
    pub dispatch_timeout: Duration,
}

#[derive(Clone, Debug, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct AvailGasRelayConfig {
    pub gas_relay_api_url: String,
    #[config(default_t = 5)]
    pub max_retries: usize,
}

#[derive(Clone, Debug, DescribeConfig, DeserializeConfig)]
pub struct AvailSecrets {
    #[config(with = FromSecretString)]
    pub seed_phrase: SeedPhrase,
    #[config(with = FromSecretString)]
    pub gas_relay_api_key: APIKey,
}
