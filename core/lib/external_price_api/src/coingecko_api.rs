use std::{collections::HashMap, str::FromStr};

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::Utc;
use reqwest;
use serde::{Deserialize, Serialize};
use zksync_config::configs::BaseTokenApiClientConfig;
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
    pub fn new(config: BaseTokenApiClientConfig) -> Self {
        let client = if let Some(api_key) = &config.api_key {
            reqwest::Client::builder()
                .default_headers(reqwest::header::HeaderMap::from_iter(std::iter::once((
                    reqwest::header::HeaderName::from_static(COINGECKO_AUTH_HEADER),
                    reqwest::header::HeaderValue::from_str(api_key)
                        .expect("Failed to create header value"),
                ))))
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

    async fn get_token_price_by_address(self: &Self, address: Address) -> anyhow::Result<f64> {
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

    async fn get_token_price_by_id(self: &Self, id: String) -> anyhow::Result<f64> {
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
    fn get_price(self: &Self, address: &String, currency: &String) -> Option<&f64> {
        self.prices
            .get(address)
            .and_then(|price| price.get(currency))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use httpmock::MockServer;
    use zksync_types::Address;

    use crate::{
        address_to_string,
        coingecko_api::{CoinGeckoPriceAPIClient, COINGECKO_AUTH_HEADER},
        tests::tests::{
            add_mock, base_token_price_not_found_test, eth_price_not_found_test, happy_day_test,
            no_base_token_price_404_test, no_eth_price_404_test, server_url,
        },
        PriceAPIClient,
    };

    fn add_mock_by_id(server: &MockServer, id: String, price: f64, api_key: Option<String>) {
        let mut params = HashMap::new();
        params.insert("ids".to_string(), id.clone());
        params.insert("vs_currencies".to_string(), "usd".to_string());
        add_mock(
            server,
            httpmock::Method::GET,
            "/api/v3/simple/price".to_string(),
            params,
            200,
            format!("{{\"{}\":{{\"usd\":{}}}}}", &id, price),
            COINGECKO_AUTH_HEADER.to_string(),
            api_key,
        );
    }

    fn add_mock_by_address(
        server: &MockServer,
        // use string explicitly to verify that conversion of the address to string works as expected
        address: String,
        price: f64,
        api_key: Option<String>,
    ) {
        let mut params = HashMap::new();
        params.insert("contract_addresses".to_string(), address.clone());
        params.insert("vs_currencies".to_string(), "usd".to_string());
        add_mock(
            server,
            httpmock::Method::GET,
            "/api/v3/simple/token_price/ethereum".to_string(),
            params,
            200,
            format!("{{\"{}\":{{\"usd\":{}}}}}", address, price),
            COINGECKO_AUTH_HEADER.to_string(),
            api_key,
        );
    }

    fn happy_day_setup(
        server: &MockServer,
        api_key: Option<String>,
        address: Address,
        base_token_price: f64,
        eth_price: f64,
    ) -> Box<dyn PriceAPIClient> {
        add_mock_by_address(
            &server,
            address_to_string(&address),
            base_token_price,
            api_key.clone(),
        );
        add_mock_by_id(&server, "ethereum".to_string(), eth_price, api_key.clone());
        Box::new(CoinGeckoPriceAPIClient::new(
            server_url(&server),
            api_key.clone(),
            std::time::Duration::from_secs(5),
        ))
    }

    #[tokio::test]
    async fn test_happy_day_with_api_key() {
        happy_day_test(Some("test".to_string()), happy_day_setup).await
    }

    #[tokio::test]
    async fn test_happy_day_with_no_api_key() {
        happy_day_test(None, happy_day_setup).await
    }

    #[tokio::test]
    async fn test_no_eth_price_404() {
        no_eth_price_404_test(
            None,
            |server: &MockServer,
             api_key: Option<String>,
             address: Address,
             _base_token_price: f64,
             _eth_price: f64|
             -> Box<dyn PriceAPIClient> {
                add_mock_by_address(&server, address_to_string(&address), 198.9, None);
                Box::new(CoinGeckoPriceAPIClient::new(
                    server_url(&server),
                    api_key,
                    std::time::Duration::from_secs(5),
                ))
            },
        )
        .await;
    }

    #[tokio::test]
    async fn test_eth_price_not_found() {
        eth_price_not_found_test(
            None,
            |server: &MockServer,
             api_key: Option<String>,
             address: Address,
             _base_token_price: f64,
             _eth_price: f64|
             -> Box<dyn PriceAPIClient> {
                add_mock_by_address(&server, address_to_string(&address), 198.9, None);
                let mut params = HashMap::new();
                params.insert("ids".to_string(), "ethereum".to_string());
                params.insert("vs_currencies".to_string(), "usd".to_string());
                add_mock(
                    &server,
                    httpmock::Method::GET,
                    "/api/v3/simple/price".to_string(),
                    params,
                    200,
                    "{}".to_string(),
                    COINGECKO_AUTH_HEADER.to_string(),
                    api_key.clone(),
                );
                Box::new(CoinGeckoPriceAPIClient::new(
                    server_url(&server),
                    api_key,
                    std::time::Duration::from_secs(5),
                ))
            },
        )
        .await;
    }

    #[tokio::test]
    async fn test_no_base_token_price_404() {
        no_base_token_price_404_test(
            None,
            |server: &MockServer,
             api_key: Option<String>,
             _address: Address,
             _base_token_price: f64,
             _eth_price: f64|
             -> Box<dyn PriceAPIClient> {
                add_mock_by_id(&server, "ethereum".to_string(), 29.5, None);
                Box::new(CoinGeckoPriceAPIClient::new(
                    server_url(&server),
                    api_key,
                    std::time::Duration::from_secs(5),
                ))
            },
        )
        .await;
    }

    #[tokio::test]
    async fn test_base_token_price_not_found() {
        base_token_price_not_found_test(
            None,
            |server: &MockServer,
             api_key: Option<String>,
             address: Address,
             _base_token_price: f64,
             _eth_price: f64|
             -> Box<dyn PriceAPIClient> {
                add_mock_by_id(&server, "ethereum".to_string(), 29.5, None);
                let mut params = HashMap::new();
                params.insert(
                    "contract_addresses".to_string(),
                    address_to_string(&address),
                );
                params.insert("vs_currencies".to_string(), "usd".to_string());
                add_mock(
                    &server,
                    httpmock::Method::GET,
                    "/api/v3/simple/token_price/ethereum".to_string(),
                    params,
                    200,
                    "{}".to_string(),
                    COINGECKO_AUTH_HEADER.to_string(),
                    api_key.clone(),
                );
                Box::new(CoinGeckoPriceAPIClient::new(
                    server_url(&server),
                    api_key,
                    std::time::Duration::from_secs(5),
                ))
            },
        )
        .await;
    }
}
