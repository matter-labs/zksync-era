use std::time::Duration;

use serde::Deserialize;

pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExternalPriceApiClientConfig {
    pub source: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[serde(default = "ExternalPriceApiClientConfig::default_timeout")]
    pub client_timeout_ms: u64,
    /// Forced conversion ratio. Only used with the ForcedPriceClient.
    pub forced_numerator: Option<u64>,
    pub forced_denominator: Option<u64>,
    pub forced_fluctuation: Option<u32>,
}

impl ExternalPriceApiClientConfig {
    fn default_timeout() -> u64 {
        DEFAULT_TIMEOUT_MS
    }

    pub fn client_timeout(&self) -> Duration {
        Duration::from_millis(self.client_timeout_ms)
    }
}
