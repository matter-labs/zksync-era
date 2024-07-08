use std::{collections::HashMap, str::FromStr};

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::Utc;
use reqwest;
use serde::{Deserialize, Serialize};
use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_types::{base_token_ratio::BaseTokenAPIPrice, Address};

use crate::{address_to_string, PriceAPIClient};

#[derive(Debug)]
pub struct CoinGeckoPriceAPIClient {
    base_url: url::Url,
    client: reqwest::Client,
}

const COINGECKO_AUTH_HEADER: &str = "x-cg-pro-api-key";
const ETH_ID: &str = "ethereum";
const USD_ID: &str = "usd";

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

        Self {
            base_url: config.base_url(),
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
                    address_str, USD_ID
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
        match cg_response.get_price(&address_str, &USD_ID.to_string()) {
            Some(&price) => Ok(price),
            None => Err(anyhow::anyhow!(
                "Price not found for token: {}",
                address_str
            )),
        }
    }

    async fn get_token_price_by_id(&self, id: String) -> anyhow::Result<f64> {
        let price_url = self
            .base_url
            .join(format!("/api/v3/simple/price?ids={}&vs_currencies={}", id, USD_ID).as_str())
            .expect("Failed to join URL path");

        let response = self.client.get(price_url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Http error while fetching token price. Status: {}, token_id: {}, msg: {}",
                response.status(),
                id,
                response.text().await.unwrap_or(String::new())
            ));
        }
        let cg_response = response.json::<CoinGeckoPriceResponse>().await?;
        match cg_response.get_price(&id, &USD_ID.to_string()) {
            Some(&price) => Ok(price),
            None => Err(anyhow::anyhow!("Price not found for token: {}", id)),
        }
    }
}

#[async_trait]
impl PriceAPIClient for CoinGeckoPriceAPIClient {
    async fn fetch_prices(&self, token_address: Address) -> anyhow::Result<BaseTokenAPIPrice> {
        let token_usd_price = self.get_token_price_by_address(token_address).await?;
        let eth_usd_price = self.get_token_price_by_id(ETH_ID.to_string()).await?;
        return Ok(BaseTokenAPIPrice {
            base_token_price: BigDecimal::from_str(&token_usd_price.to_string())?,
            eth_price: BigDecimal::from_str(&eth_usd_price.to_string())?,
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
