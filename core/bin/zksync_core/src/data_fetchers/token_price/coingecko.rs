use std::{collections::HashMap, str::FromStr};

use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use futures::try_join;
use itertools::Itertools;
use num::{rational::Ratio, BigUint};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};

use zksync_config::FetcherConfig;
use zksync_types::{
    tokens::{TokenPrice, ETHEREUM_ADDRESS},
    Address,
};
use zksync_utils::UnsignedRatioSerializeAsDecimal;

use crate::data_fetchers::error::ApiFetchError;

use super::FetcherImpl;

#[derive(Debug, Clone)]
pub struct CoinGeckoFetcher {
    client: Client,
    addr: Url,
}

impl CoinGeckoFetcher {
    pub fn new(config: &FetcherConfig) -> Self {
        Self {
            client: Client::new(),
            addr: Url::from_str(&config.token_price.url).expect("failed parse CoinGecko URL"),
        }
    }

    pub async fn fetch_erc20_token_prices(
        &self,
        tokens: &[Address],
    ) -> Result<HashMap<Address, CoinGeckoTokenPrice>, ApiFetchError> {
        let token_price_url = self
            .addr
            .join("api/v3/simple/token_price/ethereum")
            .expect("failed to join URL path");

        let mut token_prices = HashMap::new();
        let mut fetching_interval = tokio::time::interval(tokio::time::Duration::from_secs(1));
        // Splitting is needed to avoid 'Request-URI Too Large' error.
        for tokens_chunk in tokens.chunks(10) {
            fetching_interval.tick().await;
            let comma_separated_token_addresses = tokens_chunk
                .iter()
                .map(|token_addr| format!("{:#x}", token_addr))
                .join(",");

            let token_prices_chunk = self
                .client
                .get(token_price_url.clone())
                .query(&[
                    (
                        "contract_addresses",
                        comma_separated_token_addresses.as_str(),
                    ),
                    ("vs_currencies", "usd"),
                    ("include_last_updated_at", "true"),
                    ("include_24hr_change", "true"),
                ])
                .send()
                .await
                .map_err(|err| {
                    ApiFetchError::ApiUnavailable(format!("{} , Error: {}", token_price_url, err))
                })?
                .json::<HashMap<Address, CoinGeckoTokenPrice>>()
                .await
                .map_err(|err| ApiFetchError::UnexpectedJsonFormat(err.to_string()))?;
            token_prices.extend(token_prices_chunk);
        }

        Ok(token_prices)
    }

    pub async fn fetch_ethereum_price(&self) -> Result<CoinGeckoTokenPrice, ApiFetchError> {
        let coin_price_url = self
            .addr
            .join("api/v3/simple/price")
            .expect("failed to join URL path");

        let mut token_prices = self
            .client
            .get(coin_price_url.clone())
            .query(&[
                ("ids", "ethereum"),
                ("vs_currencies", "usd"),
                ("include_last_updated_at", "true"),
                ("include_24hr_change", "true"),
            ])
            .send()
            .await
            .map_err(|err| {
                ApiFetchError::ApiUnavailable(format!("{} , Error: {}", coin_price_url, err))
            })?
            .json::<HashMap<String, CoinGeckoTokenPrice>>()
            .await
            .map_err(|err| ApiFetchError::UnexpectedJsonFormat(err.to_string()))?;

        let eth_token_price = token_prices
            .remove("ethereum")
            .ok_or_else(|| ApiFetchError::Other("Failed to get ether price".to_string()))?;

        Ok(eth_token_price)
    }
}

#[async_trait]
impl FetcherImpl for CoinGeckoFetcher {
    async fn fetch_token_price(
        &self,
        tokens: &[Address],
    ) -> Result<HashMap<Address, TokenPrice>, ApiFetchError> {
        let token_prices = {
            // We have to find out the ether price separately from the erc20 tokens,
            // so we will launch requests concurrently
            if tokens.contains(&ETHEREUM_ADDRESS) {
                let (mut token_prices, ethereum_price) = try_join!(
                    self.fetch_erc20_token_prices(tokens),
                    self.fetch_ethereum_price(),
                )?;
                token_prices.insert(ETHEREUM_ADDRESS, ethereum_price);

                token_prices
            } else {
                self.fetch_erc20_token_prices(tokens).await?
            }
        };

        let result = token_prices
            .into_iter()
            .map(|(address, coingecko_token_price)| {
                let usd_price = coingecko_token_price.usd;

                let last_updated = {
                    let naive_last_updated =
                        NaiveDateTime::from_timestamp_opt(coingecko_token_price.last_updated_at, 0)
                            .unwrap();
                    DateTime::<Utc>::from_utc(naive_last_updated, Utc)
                };

                let token_price = TokenPrice {
                    usd_price,
                    last_updated,
                };

                (address, token_price)
            })
            .collect();

        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoinGeckoTokenPrice {
    /// timestamp (milliseconds)
    pub last_updated_at: i64,
    pub usd_24h_change: Option<f64>,
    #[serde(with = "UnsignedRatioSerializeAsDecimal")]
    pub usd: Ratio<BigUint>,
}

#[tokio::test]
#[ignore]
async fn test_fetch_coingecko_prices() {
    let mut config = FetcherConfig::from_env();
    config.token_price.url = "https://api.coingecko.com".to_string();

    let fetcher = CoinGeckoFetcher::new(&config);

    let tokens = vec![
        ETHEREUM_ADDRESS,
        Address::from_str("6b175474e89094c44da98b954eedeac495271d0f").expect("DAI"),
        Address::from_str("1f9840a85d5af5bf1d1762f925bdaddc4201f984").expect("UNI"),
        Address::from_str("514910771af9ca656af840dff83e8264ecf986ca").expect("LINK"),
    ];

    let token_prices = fetcher
        .fetch_token_price(&tokens)
        .await
        .expect("failed get tokens price");
    assert_eq!(
        token_prices.len(),
        tokens.len(),
        "not all data was received"
    );
    for token_address in tokens {
        assert!(token_prices.get(&token_address).is_some());
    }
}
