use std::{collections::HashMap, str::FromStr};

use async_trait::async_trait;
use chrono::Utc;
use serde::Deserialize;
use tokio::sync::RwLock;
use url::Url;
use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_types::{base_token_ratio::BaseTokenAPIRatio, Address};

use crate::{utils::get_fraction, PriceAPIClient};

const CMC_AUTH_HEADER: &str = "x-cmc_pro_api_key";
const DEFAULT_CMC_API_URL: &str = "https://pro-api.coinmarketcap.com";
// it's safe to have id hardcoded as they are stable as claimed by CMC
// const CMC_ETH_ID: i32 = 1027;
const SUPPORT_TOKENS_ONLY_ON_CMC_PLATFORM_ID: i32 = 1; // 1 = Ethereum

#[derive(Debug)]
pub struct CMCPriceAPIClient {
    base_url: Url,
    client: reqwest::Client,
    cache_token_id_by_address: RwLock<HashMap<Address, i32>>,
}

impl CMCPriceAPIClient {
    pub fn new(config: ExternalPriceApiClientConfig) -> Self {
        let client = if let Some(api_key) = &config.api_key {
            use reqwest::header::{HeaderMap, HeaderName, HeaderValue};

            let mut headers = HeaderMap::new();
            headers.insert(
                HeaderName::from_static(CMC_AUTH_HEADER),
                HeaderValue::from_str(api_key).expect("Failed to create header value"),
            );

            reqwest::Client::builder().default_headers(headers)
        } else {
            reqwest::Client::builder()
        }
        .timeout(config.client_timeout())
        .build()
        .expect("Failed to build reqwest client");

        let base_url = config.base_url.unwrap_or(DEFAULT_CMC_API_URL.to_string());

        Self {
            base_url: Url::parse(&base_url).expect("Failed to parse CoinMarketCap API URL"),
            client,
            cache_token_id_by_address: RwLock::new(HashMap::new()),
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
                if platform.id == SUPPORT_TOKENS_ONLY_ON_CMC_PLATFORM_ID
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
    // last_updated: chrono::DateTime<Utc>, // TODO: Recency?
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
impl PriceAPIClient for CMCPriceAPIClient {
    async fn fetch_ratio(&self, token_address: Address) -> anyhow::Result<BaseTokenAPIRatio> {
        let base_token_in_eth = self.get_token_price_by_address(token_address).await?;
        let (numerator, denominator) = get_fraction(base_token_in_eth);

        return Ok(BaseTokenAPIRatio {
            numerator,
            denominator,
            ratio_timestamp: Utc::now(), // TODO: Should this be now (as written), or should it be the time returned by the API?
        });
    }
}

#[cfg(test)]
mod irl_tests {
    use super::*;

    #[tokio::test]
    #[ignore = "run manually; specify CoinMarketCap API key in env var CMC_API_KEY"]
    async fn test() {
        let client = CMCPriceAPIClient::new(ExternalPriceApiClientConfig {
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

/*
#[cfg(test)]
mod tests {
    use std::{collections::HashMap, str::FromStr};

    use bigdecimal::BigDecimal;
    use httpmock::{Mock, MockServer};
    use zksync_types::{base_token_price::BaseTokenAPIPrice, Address};

    use crate::{
        address_to_string,
        cmc_api::{CMCPriceAPIClient, CMC_AUTH_HEADER, CMC_ETH_ID},
        tests::tests::{
            add_mock, base_token_price_not_found_test, eth_price_not_found_test, happy_day_test,
            no_base_token_price_404_test, no_eth_price_404_test, server_url,
        },
        PriceAPIClient,
    };

    const TEST_API_KEY: &str = "test";

    fn mock_crypto_map<'a>(
        server: &'a MockServer,
        address: &'a Address,
        mock_id: &'a String,
    ) -> Mock<'a> {
        let address_str = address_to_string(address);
        let body = format!(
            r#"{{
            "data": [
                {{
                  "id": 9999,
                  "platform": {{
                    "name": "Ethereum2",
                    "token_address": "{}"
                  }}
                }},
                {{
                  "id": {},
                  "platform": {{
                    "name": "Ethereum",
                    "token_address": "{}"
                  }}
                }}
            ]
         }}"#,
            address_str, mock_id, address_str
        );
        add_mock(
            server,
            httpmock::Method::GET,
            "/v1/cryptocurrency/map".to_string(),
            HashMap::new(),
            200,
            body,
            CMC_AUTH_HEADER.to_string(),
            Some(TEST_API_KEY.to_string()),
        )
    }

    fn add_mock_by_id<'a>(
        server: &'a MockServer,
        id: &'a String,
        price: &'a String,
        currency: &'a String,
    ) -> Mock<'a> {
        let body = format!(
            r#"{{
          "data": {{
            "{}": {{
              "quote": {{
                "{}": {{
                  "price": {}
                }}
              }}
            }}
          }}
        }}"#,
            id, currency, price
        );
        let mut params = HashMap::new();
        params.insert("id".to_string(), id.clone());
        add_mock(
            server,
            httpmock::Method::GET,
            "/v1/cryptocurrency/quotes/latest".to_string(),
            params,
            200,
            body,
            CMC_AUTH_HEADER.to_string(),
            Some(TEST_API_KEY.to_string()),
        )
    }

    fn happy_day_setup(
        server: &MockServer,
        api_key: Option<String>,
        address: Address,
        base_token_price: f64,
        eth_price: f64,
    ) -> Box<dyn PriceAPIClient> {
        let id = "50".to_string();
        let currency = "USD".to_string();
        mock_crypto_map(server, &address, &id);
        add_mock_by_id(server, &id, &base_token_price.to_string(), &currency);
        add_mock_by_id(
            server,
            &CMC_ETH_ID.to_string(),
            &eth_price.to_string(),
            &currency,
        );
        Box::new(CMCPriceAPIClient::new(
            server_url(&server),
            api_key.unwrap(),
            reqwest::Client::new(),
        ))
    }

    #[tokio::test]
    async fn test_happy_day() {
        happy_day_test(Some(TEST_API_KEY.to_string()), happy_day_setup).await
    }

    #[tokio::test]
    async fn test_no_token_id() {
        let server = MockServer::start();
        let address_str = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
        let address = Address::from_str(address_str).unwrap();
        let id = "50".to_string();
        let base_token_price = 198.9;
        let eth_price = 3000.0;
        let currency = "USD".to_string();

        // the response will be missing the token that we are seeking for
        mock_crypto_map(
            &server,
            &Address::from_str("0x3Bad7800d9149B53Cba5da927E6449e4A3487a1F").unwrap(),
            &"123".to_string(),
        );
        add_mock_by_id(&server, &id, &base_token_price.to_string(), &currency);
        add_mock_by_id(
            &server,
            &CMC_ETH_ID.to_string(),
            &eth_price.to_string(),
            &currency,
        );

        let mut client = CMCPriceAPIClient::new(
            server_url(&server),
            TEST_API_KEY.to_string(),
            reqwest::Client::new(),
        );
        let api_price = client.fetch_price(address).await;

        assert!(api_price.is_err());
        let msg = api_price.err().unwrap().to_string();
        assert_eq!(
            "Token id not found for address 0x1f9840a85d5af5bf1d1762f925bdaddc4201f984".to_string(),
            msg
        )
    }

    #[tokio::test]
    async fn should_reuse_token_id_from_map() {
        let server = MockServer::start();
        let address_str = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
        let address = Address::from_str(address_str).unwrap();
        let base_token_price = 198.9;
        let eth_price = 3000.0;
        let id = "50".to_string();
        let currency = "USD".to_string();

        let cm_mock = mock_crypto_map(&server, &address, &id);
        add_mock_by_id(&server, &id, &base_token_price.to_string(), &currency);
        add_mock_by_id(
            &server,
            &CMC_ETH_ID.to_string(),
            &eth_price.to_string(),
            &currency,
        );
        let mut client = CMCPriceAPIClient::new(
            server_url(&server),
            TEST_API_KEY.to_string(),
            reqwest::Client::new(),
        );

        client.fetch_price(address).await.unwrap();
        let api_price = client.fetch_price(address).await.unwrap();

        assert_eq!(
            BaseTokenAPIPrice {
                base_token_price: BigDecimal::from_str(&base_token_price.to_string()).unwrap(),
                eth_price: BigDecimal::from_str(&eth_price.to_string()).unwrap(),
                ratio_timestamp: api_price.ratio_timestamp,
            },
            api_price
        );
        // crypto map should be fetched only once
        assert_eq!(1, cm_mock.hits());
    }

    #[tokio::test]
    async fn test_no_eth_price_404() {
        no_eth_price_404_test(
            Some(TEST_API_KEY.to_string()),
            |server: &MockServer,
             api_key: Option<String>,
             address: Address,
             _base_token_price: f64,
             _eth_price: f64|
             -> Box<dyn PriceAPIClient> {
                let id = "50".to_string();
                mock_crypto_map(&server, &address, &id);
                add_mock_by_id(&server, &id, &"123".to_string(), &"USD".to_string());

                Box::new(CMCPriceAPIClient::new(
                    server_url(&server),
                    api_key.unwrap(),
                    reqwest::Client::new(),
                ))
            },
        )
        .await;
    }

    #[tokio::test]
    async fn test_eth_price_not_found() {
        eth_price_not_found_test(
            Some(TEST_API_KEY.to_string()),
            |server: &MockServer,
             api_key: Option<String>,
             address: Address,
             _base_token_price: f64,
             _eth_price: f64|
             -> Box<dyn PriceAPIClient> {
                let id = "50".to_string();
                mock_crypto_map(&server, &address, &id);
                add_mock_by_id(&server, &id, &"123".to_string(), &"USD".to_string());
                let mut params = HashMap::new();
                params.insert("id".to_string(), CMC_ETH_ID.to_string());
                add_mock(
                    server,
                    httpmock::Method::GET,
                    "/v1/cryptocurrency/quotes/latest".to_string(),
                    params,
                    200,
                    "{}".to_string(),
                    CMC_AUTH_HEADER.to_string(),
                    Some(TEST_API_KEY.to_string()),
                );
                Box::new(CMCPriceAPIClient::new(
                    server_url(&server),
                    api_key.unwrap(),
                    reqwest::Client::new(),
                ))
            },
        )
        .await;
    }

    #[tokio::test]
    async fn test_no_base_token_price_404() {
        no_base_token_price_404_test(
            Some(TEST_API_KEY.to_string()),
            |server: &MockServer,
             api_key: Option<String>,
             address: Address,
             _base_token_price: f64,
             _eth_price: f64|
             -> Box<dyn PriceAPIClient> {
                mock_crypto_map(&server, &address, &"55".to_string());
                add_mock_by_id(
                    &server,
                    &CMC_ETH_ID.to_string(),
                    &"3900.12".to_string(),
                    &"USD".to_string(),
                );
                Box::new(CMCPriceAPIClient::new(
                    server_url(&server),
                    api_key.unwrap(),
                    reqwest::Client::new(),
                ))
            },
        )
        .await;
    }

    #[tokio::test]
    async fn test_base_token_price_not_found() {
        base_token_price_not_found_test(
            Some(TEST_API_KEY.to_string()),
            |server: &MockServer,
             api_key: Option<String>,
             address: Address,
             _base_token_price: f64,
             _eth_price: f64|
             -> Box<dyn PriceAPIClient> {
                let id = "55".to_string();
                mock_crypto_map(&server, &address, &id);
                add_mock_by_id(
                    &server,
                    &CMC_ETH_ID.to_string(),
                    &"3900.12".to_string(),
                    &"USD".to_string(),
                );
                let mut params = HashMap::new();
                params.insert("id".to_string(), id);
                add_mock(
                    server,
                    httpmock::Method::GET,
                    "/v1/cryptocurrency/quotes/latest".to_string(),
                    params,
                    200,
                    "{}".to_string(),
                    CMC_AUTH_HEADER.to_string(),
                    Some(TEST_API_KEY.to_string()),
                );
                Box::new(CMCPriceAPIClient::new(
                    server_url(&server),
                    api_key.unwrap(),
                    reqwest::Client::new(),
                ))
            },
        )
        .await;
    }
}
 */
