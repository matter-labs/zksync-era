use std::{collections::HashMap, str::FromStr};

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use tokio::sync::RwLock;
use url::Url;
use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_types::{base_token_ratio::BaseTokenAPIRatio, Address};

use crate::{utils::get_fraction, PriceAPIClient};

const AUTH_HEADER: &str = "x-cmc_pro_api_key";
const DEFAULT_API_URL: &str = "https://pro-api.coinmarketcap.com";
const ALLOW_TOKENS_ONLY_ON_PLATFORM_ID: i32 = 1; // 1 = Ethereum

#[derive(Debug)]
pub struct CmcPriceApiClient {
    base_url: Url,
    client: reqwest::Client,
    cache_token_id_by_address: RwLock<HashMap<Address, i32>>,
}

impl CmcPriceApiClient {
    pub fn new(config: ExternalPriceApiClientConfig) -> Self {
        let client = if let Some(api_key) = &config.api_key {
            use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

            let default_headers = HeaderMap::from_iter([(
                HeaderName::from_static(AUTH_HEADER),
                HeaderValue::from_str(api_key).expect("Failed to create header value"),
            )]);

            reqwest::Client::builder().default_headers(default_headers)
        } else {
            reqwest::Client::builder()
        }
        .timeout(config.client_timeout())
        .build()
        .expect("Failed to build reqwest client");

        let base_url = config.base_url.unwrap_or(DEFAULT_API_URL.to_string());
        let base_url = Url::parse(&base_url).expect("Failed to parse CoinMarketCap API URL");

        Self {
            base_url,
            client,
            cache_token_id_by_address: RwLock::default(),
        }
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        self.client
            .get(self.base_url.join(path).expect("Failed to join URL path"))
    }

    async fn get_token_id(&self, address: Address) -> anyhow::Result<i32> {
        if let Some(x) = self.cache_token_id_by_address.read().await.get(&address) {
            return Ok(*x);
        }
        // drop read lock

        let response = self.get("/v1/cryptocurrency/map").send().await?;
        let status = response.status();
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Http error while fetching token id. Status: {status}, token: {address}, msg: {}",
                response.text().await.unwrap_or_default(),
            ));
        }

        let parsed = response.json::<V1CryptocurrencyMapResponse>().await?;
        for token_info in parsed.data {
            if let Some(platform) = token_info.platform {
                if platform.id == ALLOW_TOKENS_ONLY_ON_PLATFORM_ID
                    && Address::from_str(&platform.token_address).is_ok_and(|a| a == address)
                {
                    self.cache_token_id_by_address
                        .write()
                        .await
                        .insert(address, token_info.id);
                    return Ok(token_info.id);
                }
            }
        }

        Err(anyhow::anyhow!("Token ID not found for address {address}"))
    }

    async fn get_token_price_by_address(&self, address: Address) -> anyhow::Result<f64> {
        let id = self.get_token_id(address).await?;
        self.get_token_price_by_id(id).await
    }

    async fn get_token_price_by_id(&self, id: i32) -> anyhow::Result<f64> {
        let response = self
            .get("/v2/cryptocurrency/quotes/latest")
            .query(&[("id", id)])
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Http error while fetching token price. Status: {status}, token: {id}, msg: {}",
                response.text().await.unwrap_or("".to_string())
            ));
        }

        response
            .json::<V2CryptocurrencyQuotesLatestResponse>()
            .await?
            .data
            .get(&id)
            .and_then(|data| data.quote.get("USD"))
            .map(|mq| mq.price)
            .ok_or_else(|| anyhow::anyhow!("Price not found for token: {id}"))
    }
}

#[derive(Debug, Deserialize)]
struct V2CryptocurrencyQuotesLatestResponse {
    data: HashMap<i32, CryptocurrencyQuoteObject>,
}

#[derive(Debug, Deserialize)]
struct CryptocurrencyQuoteObject {
    // #[serde(flatten)]
    // cryptocurrency_object: CryptocurrencyObject,
    quote: HashMap<String, MarketQuote>,
}

#[derive(Debug, Deserialize)]
struct MarketQuote {
    price: f64,
}

#[derive(Debug, Deserialize)]
struct V1CryptocurrencyMapResponse {
    data: Vec<CryptocurrencyObject>,
}

#[derive(Debug, Deserialize)]
struct CryptocurrencyObject {
    id: i32,
    // name: String,
    // symbol: String,
    // slug: String,
    // is_active: u8, // TODO: This field is available, should we at least emit a warning if the listing is not marked as active?
    platform: Option<CryptocurrencyPlatform>,
}

#[derive(Debug, Deserialize)]
struct CryptocurrencyPlatform {
    id: i32,
    // name: String,
    token_address: String,
}

#[async_trait]
impl PriceAPIClient for CmcPriceApiClient {
    async fn fetch_ratio(&self, token_address: Address) -> anyhow::Result<BaseTokenAPIRatio> {
        let base_token_in_eth = self.get_token_price_by_address(token_address).await?;
        let (numerator, denominator) = get_fraction(base_token_in_eth);

        return Ok(BaseTokenAPIRatio {
            numerator,
            denominator,
            ratio_timestamp: Utc::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "run manually (accesses network); specify CoinMarketCap API key in env var CMC_API_KEY"]
    async fn test() {
        let client = CmcPriceApiClient::new(ExternalPriceApiClientConfig {
            api_key: Some(std::env::var("CMC_API_KEY").unwrap()),
            base_url: None,
            client_timeout_ms: 5000,
            source: String::new(),
            forced: None,
        });

        let tether: Address = "0xdac17f958d2ee523a2206206994597c13d831ec7"
            .parse()
            .unwrap();

        let r = client.get_token_price_by_address(tether).await.unwrap();

        assert!((r - 1f64).abs() < 0.001, "USDT lost its peg");

        println!("{r}");
    }
}
