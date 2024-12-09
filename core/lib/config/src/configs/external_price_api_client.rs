use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ForcedPriceClientConfig {
    /// Forced conversion ratio
    pub numerator: Option<u64>,
    pub denominator: Option<u64>,
    /// Forced fluctuation. It defines how much percent the ratio should fluctuate from its forced
    /// value. If it's None or 0, then the ForcedPriceClient will return the same quote every time
    /// it's called. Otherwise, ForcedPriceClient will return quote with numerator +/- fluctuation %.
    pub fluctuation: Option<u32>,
    /// In order to smooth out fluctuation, consecutive values returned by forced client will not
    /// differ more than next_value_fluctuation percent.
    #[config(default_t = 3)]
    pub next_value_fluctuation: u32,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ExternalPriceApiClientConfig {
    #[config(default_t = "noop".into())]
    pub source: String,
    pub base_url: Option<String>,
    pub api_key: Option<String>,
    #[config(default_t = Duration::from_secs(10), with = TimeUnit::Millis)]
    pub client_timeout_ms: Duration,
    #[config(nest)]
    pub forced: Option<ForcedPriceClientConfig>,
}
