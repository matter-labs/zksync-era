use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use reqwest;
use serde::{Deserialize, Serialize};
use url::Url;
use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_types::{base_token_ratio::BaseTokenAPIRatio, Address};

use crate::{address_to_string, utils::get_fraction, PriceAPIClient};

#[derive(Debug)]
pub struct CoinGeckoPriceAPIClient {
    base_url: Url,
    client: reqwest::Client,
}

const DEFAULT_COINGECKO_API_URL: &str = "https://pro-api.coingecko.com";
const COINGECKO_AUTH_HEADER: &str = "x-cg-pro-api-key";
const ETH_ID: &str = "eth";

impl CoinGeckoPriceAPIClient {
    pub fn new(config: ExternalPriceApiClientConfig) -> Self {
        let client = if let Some(api_key) = &config.api_key {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::HeaderName::from_static(COINGECKO_AUTH_HEADER),
                reqwest::header::HeaderValue::from_str(api_key)
                    .expect("Failed to create header value"),
            );

            reqwest::Client::builder()
                .default_headers(headers)
                .timeout(config.client_timeout())
                .build()
                .expect("Failed to build reqwest client")
        } else {
            reqwest::Client::new()
        };

        let base_url = config
            .base_url
            .unwrap_or(DEFAULT_COINGECKO_API_URL.to_string());

        Self {
            base_url: Url::parse(&base_url).expect("Failed to parse CoinGecko URL"),
            client,
        }
    }

    async fn get_token_price_by_address(&self, address: Address) -> anyhow::Result<f64> {
        let address_str = address_to_string(&address);
        let price_url = self
            .base_url
            .join(
                format!(
                    "/api/v3/simple/token_price/ethereum?contract_addresses={}&vs_currencies={}",
                    address_str, ETH_ID
                )
                .as_str(),
            )
            .expect("failed to join URL path");

        let response = self.client.get(price_url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Http error while fetching token price. Status: {}, token_addr: {}, msg: {}",
                response.status(),
                address_str,
                response.text().await.unwrap_or(String::new())
            ));
        }

        let cg_response = response.json::<CoinGeckoPriceResponse>().await?;
        match cg_response.get_price(&address_str, &ETH_ID.to_string()) {
            Some(&price) => Ok(price),
            None => Err(anyhow::anyhow!(
                "Price not found for token: {}",
                address_str
            )),
        }
    }
}

#[async_trait]
impl PriceAPIClient for CoinGeckoPriceAPIClient {
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CoinGeckoPriceResponse {
    #[serde(flatten)]
    pub(crate) prices: HashMap<String, HashMap<String, f64>>,
}

impl CoinGeckoPriceResponse {
    fn get_price(&self, address: &String, currency: &String) -> Option<&f64> {
        self.prices
            .get(address)
            .and_then(|price| price.get(currency))
    }
}
