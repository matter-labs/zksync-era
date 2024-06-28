use std::{collections::HashMap, str::FromStr};

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::Utc;
use serde::Deserialize;
use url::Url;
use zksync_types::{base_token_price::BaseTokenAPIPrice, Address};

use crate::{address_to_string, PriceAPIClient};

const CMC_AUTH_HEADER: &str = "X-CMC_PRO_API_KEY";
// it's safe to have id hardcoded as they are stable as claimed by CMC
const CMC_ETH_ID: i32 = 1027;

#[derive(Debug)]
pub struct CMCPriceAPIClient {
    base_url: Url,
    api_key: String,
    client: reqwest::Client,
    token_id_by_address: HashMap<Address, i32>,
}

impl CMCPriceAPIClient {
    fn new(base_url: Url, api_key: String, client: reqwest::Client) -> Self {
        Self {
            base_url,
            api_key,
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
                let response = self
                    .client
                    .get(url)
                    .header(CMC_AUTH_HEADER, &self.api_key)
                    .send()
                    .await?;
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

        let response = self
            .client
            .get(price_url)
            .header(CMC_AUTH_HEADER, &self.api_key)
            .send()
            .await?;
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
    async fn fetch_price(&mut self, token_address: Address) -> anyhow::Result<BaseTokenAPIPrice> {
        let token_usd_price = self.get_token_price_by_address(token_address).await?;
        let eth_usd_price = self.get_token_price_by_id(CMC_ETH_ID).await?;
        return Ok(BaseTokenAPIPrice {
            base_token_price: BigDecimal::from_str(&token_usd_price.to_string())?,
            eth_price: BigDecimal::from_str(&eth_usd_price.to_string())?,
            ratio_timestamp: Utc::now(),
        });
    }
}

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
