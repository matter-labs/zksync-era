use std::time::Duration;

// Built-in uses
// External uses
use serde::Deserialize;
// Workspace uses
// Local uses
use super::envy_load;

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

impl FetcherConfig {
    pub fn from_env() -> Self {
        Self {
            token_list: envy_load("token_list", "FETCHER_TOKEN_LIST_"),
            token_price: envy_load("token_price", "FETCHER_TOKEN_PRICE_"),
            token_trading_volume: envy_load(
                "token_trading_volume",
                "FETCHER_TOKEN_TRADING_VOLUME_",
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> FetcherConfig {
        FetcherConfig {
            token_list: SingleFetcherConfig {
                source: TokenListSource::OneInch,
                url: "http://127.0.0.1:1020".into(),
                fetching_interval: 10,
            },
            token_price: SingleFetcherConfig {
                source: TokenPriceSource::CoinGecko,
                url: "http://127.0.0.1:9876".into(),
                fetching_interval: 7,
            },
            token_trading_volume: SingleFetcherConfig {
                source: TokenTradingVolumeSource::Uniswap,
                url: "http://127.0.0.1:9975/graphql".to_string(),
                fetching_interval: 5,
            },
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            FETCHER_TOKEN_LIST_SOURCE="OneInch"
            FETCHER_TOKEN_LIST_URL="http://127.0.0.1:1020"
            FETCHER_TOKEN_LIST_FETCHING_INTERVAL="10"
            FETCHER_TOKEN_PRICE_SOURCE="CoinGecko"
            FETCHER_TOKEN_PRICE_URL="http://127.0.0.1:9876"
            FETCHER_TOKEN_PRICE_FETCHING_INTERVAL="7"
            FETCHER_TOKEN_TRADING_VOLUME_SOURCE="Uniswap"
            FETCHER_TOKEN_TRADING_VOLUME_URL="http://127.0.0.1:9975/graphql"
            FETCHER_TOKEN_TRADING_VOLUME_FETCHING_INTERVAL="5"
        "#;
        lock.set_env(config);

        let actual = FetcherConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
