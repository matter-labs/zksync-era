use std::{collections::HashMap, str::FromStr};

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use tokio::sync::RwLock;
use url::Url;
use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_types::{base_token_ratio::BaseTokenApiRatio, Address};

use crate::{address_to_string, utils::get_fraction, PriceApiClient};

const AUTH_HEADER: &str = "x-cmc_pro_api_key";
const DEFAULT_API_URL: &str = "https://pro-api.coinmarketcap.com";
const ALLOW_TOKENS_ONLY_ON_PLATFORM_ID: i32 = 1; // 1 = Ethereum
const REQUEST_QUOTE_IN_CURRENCY_ID: &str = "1027"; // 1027 = ETH

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
                    if token_info.is_active != 1 {
                        tracing::warn!(
                            "CoinMarketCap API reports token {} ({}) on platform {} ({}) is not active",
                            address_to_string(&address),
                            token_info.name,
                            platform.id,
                            platform.name,
                        );
                    }

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
            .query(&[("convert_id", REQUEST_QUOTE_IN_CURRENCY_ID)])
            .send()
            .await?;

        let status = response.status();
        if !status.is_success() {
            return Err(anyhow::anyhow!(
                "Http error while fetching token price. Status: {status}, token: {id}, msg: {}",
                response.text().await.unwrap_or_default(),
            ));
        }

        response
            .json::<V2CryptocurrencyQuotesLatestResponse>()
            .await?
            .data
            .get(&id)
            .and_then(|data| data.quote.get(REQUEST_QUOTE_IN_CURRENCY_ID))
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
    name: String,
    is_active: u8,
    platform: Option<CryptocurrencyPlatform>,
}

#[derive(Debug, Deserialize)]
struct CryptocurrencyPlatform {
    id: i32,
    name: String,
    token_address: String,
}

#[async_trait]
impl PriceApiClient for CmcPriceApiClient {
    async fn fetch_ratio(&self, token_address: Address) -> anyhow::Result<BaseTokenApiRatio> {
        let base_token_in_eth = self.get_token_price_by_address(token_address).await?;
        let (term_ether, term_base_token) = get_fraction(base_token_in_eth)?;

        return Ok(BaseTokenApiRatio {
            numerator: term_base_token,
            denominator: term_ether,
            ratio_timestamp: Utc::now(),
        });
    }
}

#[cfg(test)]
mod tests {
    use httpmock::prelude::*;
    use serde_json::json;

    use super::*;
    use crate::tests::*;

    fn make_client(server: &MockServer, api_key: Option<String>) -> Box<dyn PriceApiClient> {
        Box::new(CmcPriceApiClient::new(ExternalPriceApiClientConfig {
            source: "coinmarketcap".to_string(),
            base_url: Some(server.base_url()),
            api_key,
            client_timeout_ms: 5000,
            forced: None,
        }))
    }

    fn make_mock_server() -> MockServer {
        let mock_server = MockServer::start();
        // cryptocurrency map
        mock_server.mock(|when, then| {
            when.method(GET)
                .header_exists(AUTH_HEADER)
                .path("/v1/cryptocurrency/map");
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "status": {
                        "timestamp": "2024-09-25T11:29:38.440Z",
                        "error_code": 0,
                        "error_message": null,
                        "elapsed": 351,
                        "credit_count": 1,
                        "notice": null
                    },
                    "data": [
                        {
                            "id": 7083,
                            "rank": 26,
                            "name": "Uniswap",
                            "symbol": "UNI",
                            "slug": "uniswap",
                            "is_active": 1,
                            "first_historical_data": "2020-09-17T01:10:00.000Z",
                            "last_historical_data": "2024-09-25T11:25:00.000Z",
                            "platform": {
                                "id": 1,
                                "name": "Ethereum",
                                "symbol": "ETH",
                                "slug": "ethereum",
                                "token_address": "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984"
                            }
                        }
                    ]
                }));
        });

        // cryptocurrency quote
        mock_server.mock(|when, then| {
            // TODO: check for api authentication header
            when.method(GET)
                .header_exists(AUTH_HEADER)
                .path("/v2/cryptocurrency/quotes/latest")
                .query_param("id", "7083") // Uniswap
                .query_param("convert_id", "1027"); // Ether
            then.status(200)
                .header("content-type", "application/json")
                .json_body(json!({
                    "status": {
                        "timestamp": "2024-10-02T14:15:07.189Z",
                        "error_code": 0,
                        "error_message": null,
                        "elapsed": 39,
                        "credit_count": 1,
                        "notice": null
                    },
                    "data": {
                        "7083": {
                            "id": 7083,
                            "name": "Uniswap",
                            "symbol": "UNI",
                            "slug": "uniswap",
                            "date_added": "2020-09-17T00:00:00.000Z",
                            "tags": [],
                            "max_supply": null,
                            "circulating_supply": 600294743.71,
                            "total_supply": 1000000000,
                            "platform": {
                                "id": 1027,
                                "name": "Ethereum",
                                "symbol": "ETH",
                                "slug": "ethereum",
                                "token_address": "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984"
                            },
                            "is_active": 1,
                            "infinite_supply": false,
                            "cmc_rank": 22,
                            "is_fiat": 0,
                            "last_updated": "2024-10-02T14:13:00.000Z",
                            "quote": {
                                "1027": {
                                    "price": 0.0028306661720164175,
                                    "last_updated": "2024-10-02T14:12:00.000Z"
                                }
                            }
                        }
                    }
                }));
        });

        mock_server
    }

    #[tokio::test]
    async fn mock_happy() {
        let server = make_mock_server();
        let client = make_client(
            &server,
            Some("00000000-0000-0000-0000-000000000000".to_string()),
        );

        let token_address: Address = TEST_TOKEN_ADDRESS.parse().unwrap();

        let api_price = client.fetch_ratio(token_address).await.unwrap();

        const REPORTED_PRICE: f64 = 1_f64 / 0.0028306661720164175_f64;
        const EPSILON: f64 = 0.000001_f64 * REPORTED_PRICE;

        assert!((approximate_value(&api_price) - REPORTED_PRICE).abs() < EPSILON);
    }

    #[tokio::test]
    #[should_panic = "Request did not match any route or mock"]
    async fn mock_fail_no_api_key() {
        let server = make_mock_server();
        let client = make_client(&server, None);

        let token_address: Address = TEST_TOKEN_ADDRESS.parse().unwrap();

        client.fetch_ratio(token_address).await.unwrap();
    }

    #[tokio::test]
    #[should_panic = "Token ID not found for address"]
    async fn mock_fail_not_found() {
        let server = make_mock_server();
        let client = make_client(
            &server,
            Some("00000000-0000-0000-0000-000000000000".to_string()),
        );

        let token_address: Address = Address::random();

        client.fetch_ratio(token_address).await.unwrap();
    }

    #[tokio::test]
    #[ignore = "run manually (accesses network); specify CoinMarketCap API key in env var CMC_API_KEY"]
    async fn real_cmc_tether() {
        let client = CmcPriceApiClient::new(ExternalPriceApiClientConfig {
            api_key: Some(std::env::var("CMC_API_KEY").unwrap()),
            base_url: None,
            client_timeout_ms: 5000,
            source: "coinmarketcap".to_string(),
            forced: None,
        });

        let tether: Address = "0xdac17f958d2ee523a2206206994597c13d831ec7"
            .parse()
            .unwrap();

        let r = client.get_token_price_by_address(tether).await.unwrap();

        println!("{r}");
    }
}
