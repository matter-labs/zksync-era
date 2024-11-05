use std::time::Duration;

use serde::Deserialize;

pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;

pub const DEFAULT_FORCED_NEXT_VALUE_FLUCTUATION: u32 = 3;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ForcedPriceClientConfig {
    /// Forced conversion ratio
    pub numerator: Option<u64>,
    pub denominator: Option<u64>,
    /// Forced fluctuation. It defines how much percent the ratio should fluctuate from its forced
    /// value. If it's None or 0, then the ForcedPriceClient will return the same quote every time
    /// it's called. Otherwise, ForcedPriceClient will return quote with numerator +/- fluctuation %.
    pub fluctuation: Option<u32>,
    /// In order to smooth out fluctuation, consecutive values returned by forced client will not
    /// differ more than next_value_fluctuation percent. If it's None, a default of 3% will be applied.
    #[serde(default = "ExternalPriceApiClientConfig::default_forced_next_value_fluctuation")]
    pub next_value_fluctuation: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExternalPriceApiClientConfig {
    pub source: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[serde(default = "ExternalPriceApiClientConfig::default_timeout")]
    pub client_timeout_ms: u64,
    pub forced: Option<ForcedPriceClientConfig>,
}

impl ExternalPriceApiClientConfig {
    fn default_timeout() -> u64 {
        DEFAULT_TIMEOUT_MS
    }

    fn default_forced_next_value_fluctuation() -> u32 {
        DEFAULT_FORCED_NEXT_VALUE_FLUCTUATION
    }

    pub fn client_timeout(&self) -> Duration {
        Duration::from_millis(self.client_timeout_ms)
    }
}
