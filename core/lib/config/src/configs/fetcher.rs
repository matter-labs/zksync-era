use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum TokenListSource {
    OneInch,
    Mock,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum TokenPriceSource {
    CoinGecko,
    CoinMarketCap,
    Mock,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum TokenTradingVolumeSource {
    Uniswap,
    Mock,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct SingleFetcherConfig<TYPE> {
    /// Indicator of the API to be used for getting information.
    pub source: TYPE,
    /// URL of the API to use for fetching data. Not used for `mock` source.
    pub url: String,
    // Interval for fetching API data in seconds. Basically, how ofter do we need to poll third-part APIs.
    pub fetching_interval: u64,
}

impl<TYPE> SingleFetcherConfig<TYPE> {
    pub fn fetching_interval(&self) -> Duration {
        Duration::from_secs(self.fetching_interval)
    }
}

/// Configuration for the third-party API data fetcher.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FetcherConfig {
    pub token_list: SingleFetcherConfig<TokenListSource>,
    pub token_price: SingleFetcherConfig<TokenPriceSource>,
    pub token_trading_volume: SingleFetcherConfig<TokenTradingVolumeSource>,
}
