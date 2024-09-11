use std::time::Duration;

use serde::Deserialize;

pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ForcedPriceClientConfig {
    /// Forced conversion ratio
    pub numerator: Option<u64>,
    pub denominator: Option<u64>,
    /// Forced fluctuation. It defines how much percent numerator /
    /// denominator should fluctuate from their forced values. If it's None or 0, then ForcedPriceClient
    /// will return the same quote every time it's called. Otherwise, ForcedPriceClient will return
    /// forced_quote +/- forced_fluctuation % from its values.
    pub fluctuation: Option<u32>,
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

    pub fn client_timeout(&self) -> Duration {
        Duration::from_millis(self.client_timeout_ms)
    }
}
