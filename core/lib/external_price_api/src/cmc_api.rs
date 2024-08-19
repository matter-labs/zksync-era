use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use url::Url;
use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_types::{base_token_ratio::BaseTokenAPIRatio, Address};

use crate::{address_to_string, utils::get_fraction, PriceAPIClient};

const DEFAULT_CMC_API_URL: &str = "https://pro-api.coinmarketcap.com";
const CMC_AUTH_HEADER: &str = "X-CMC_PRO_API_KEY";
// it's safe to have id hardcoded as they are stable as claimed by CMC
const CMC_ETH_ID: i32 = 1027;

#[derive(Debug)]
pub struct CMCPriceAPIClient {
    base_url: Url,
    client: reqwest::Client,
    token_id_by_address: HashMap<Address, i32>,
}

impl CMCPriceAPIClient {
    pub fn new(config: ExternalPriceApiClientConfig) -> Self {
        let client = if let Some(api_key) = &config.api_key {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::HeaderName::from_static(CMC_AUTH_HEADER),
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

        let base_url = config.base_url.unwrap_or(DEFAULT_CMC_API_URL.to_string());

        Self {
            base_url: Url::parse(&base_url).expect("Failed to parse CoinGecko URL"),
            client,
            token_id_by_address: HashMap::new(),
        }
    }

    async fn get_token_id(self: &mut Self, address: Address) -> anyhow::Result<i32> {
        match self.token_id_by_address.get(&address) {
            Some(x) => Ok(*x),
            None => {
                let url = self
                    .base_url
                    .join("/v1/cryptocurrency/map")
                    .expect("failed to join URL path");
                let address_str = address_to_string(&address);
                let response = self.client.get(url).send().await?;
                if !response.status().is_success() {
                    return Err(anyhow::anyhow!(
                        "Http error while fetching token id. Status: {}, token: {}, msg: {}",
                        response.status(),
                        address_str,
                        response.text().await.unwrap_or("".to_string())
                    ));
                }

                let cmc_response = response.json::<CMCResponse>().await?;
                for token_info in cmc_response.data {
                    if let Some(platform) = token_info.platform {
                        if platform.name.to_ascii_lowercase() == "ethereum"
                            && platform.token_address == address_str
                        {
                            self.token_id_by_address.insert(address, token_info.id);
                            return Ok(token_info.id);
                        }
                    }
                }

                Err(anyhow::anyhow!(
                    "Token id not found for address {}",
                    address_str
                ))
            }
        }
    }

    async fn get_token_price_by_address(self: &mut Self, address: Address) -> anyhow::Result<f64> {
        let id = self.get_token_id(address).await?;
        self.get_token_price_by_id(id).await
    }

    async fn get_token_price_by_id(self: &Self, id: i32) -> anyhow::Result<f64> {
        let price_url = self
            .base_url
            .join(format!("/v1/cryptocurrency/quotes/latest?id={}", id).as_str())
            .expect("failed to join URL path");

        let response = self.client.get(price_url).send().await?;
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Http error while fetching token price. Status: {}, token: {}, msg: {}",
                response.status(),
                id,
                response.text().await.unwrap_or("".to_string())
            ));
        }

        let json_response = response.json::<serde_json::Value>().await?;
        match json_response["data"][id.to_string()]["quote"]["USD"]["price"].as_f64() {
            Some(x) => Ok(x),
            None => Err(anyhow::anyhow!("Price not found for token: {}", &id)),
        }
    }
}

#[derive(Debug, Deserialize)]
struct CMCResponse {
    data: Vec<CMCTokenInfo>,
}

#[derive(Debug, Deserialize)]
struct CMCTokenInfo {
    id: i32,
    platform: Option<CMCPlatform>,
}

#[derive(Debug, Deserialize)]
struct CMCPlatform {
    name: String,
    token_address: String,
}

#[async_trait]
impl PriceAPIClient for CMCPriceAPIClient {
    async fn fetch_ratio(&mut self, token_address: Address) -> anyhow::Result<BaseTokenAPIRatio> {
        let token_usd_price = self.get_token_price_by_address(token_address).await?;
        let eth_usd_price = self.get_token_price_by_id(CMC_ETH_ID).await?;
        let token_in_eth = token_usd_price / eth_usd_price;
        let (numerator, denominator) = get_fraction(token_in_eth);
        Ok(BaseTokenAPIRatio {
            numerator,
            denominator,
            ratio_timestamp: Utc::now(),
        })
    }
}
