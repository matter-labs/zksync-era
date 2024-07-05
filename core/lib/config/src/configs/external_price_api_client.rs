use std::time::Duration;

use serde::Deserialize;
use url::Url;

pub const DEFAULT_TIMEOUT_MS: u64 = 10_000;
pub const DEFAULT_COINGECKO_API_URL: &str = "https://pro-api.coingecko.com";

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExternalPriceApiClientConfig {
    #[serde(default = "ExternalPriceApiClientConfig::default_base_url")]
    pub base_url: String,
    pub api_key: Option<String>,
    #[serde(default = "ExternalPriceApiClientConfig::default_timeout")]
    pub client_timeout_ms: u64,
}

impl ExternalPriceApiClientConfig {
    fn default_timeout() -> u64 {
        DEFAULT_TIMEOUT_MS
    }

    fn default_base_url() -> String {
        DEFAULT_COINGECKO_API_URL.to_string()
    }

    pub fn client_timeout(&self) -> Duration {
        Duration::from_millis(self.client_timeout_ms)
    }

    pub fn base_url(&self) -> Url {
        Url::parse(self.base_url.as_str()).unwrap_or(Url::parse(DEFAULT_COINGECKO_API_URL).unwrap())
    }
}
