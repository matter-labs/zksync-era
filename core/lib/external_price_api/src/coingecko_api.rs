use std::{collections::HashMap, str::FromStr};

use async_trait::async_trait;
use bigdecimal::BigDecimal;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use url::Url;
use zksync_types::{base_token_price::BaseTokenAPIPrice, Address};

use crate::{address_to_string, PriceAPIClient};

#[derive(Debug)]
pub struct CoinGeckoPriceAPIClient {
    base_url: Url,
    api_key: Option<String>,
    client: reqwest::Client,
}

const CG_AUTH_HEADER: &str = "x-cg-pro-api-key";

impl CoinGeckoPriceAPIClient {
    async fn get_token_price_by_address(
        self: &Self,
        token_address: Address,
    ) -> anyhow::Result<f64> {
        let vs_currency = "usd";
        let token_price_url = self
            .base_url
            .join(
                format!(
                    "/api/v3/simple/token_price/ethereum?contract_addresses={}&vs_currencies={}",
                    address_to_string(token_address),
                    vs_currency
                )
                .as_str(),
            )
            .expect("failed to join URL path");

        let mut builder = self.client.get(token_price_url);

        if let Some(x) = &self.api_key {
            builder = builder.header(CG_AUTH_HEADER, x);
        }

        let response = builder
            .send()
            .await?
            .json::<CoinGeckoPriceResponse>()
            .await?;
        match response.get_price(
            &address_to_string(token_address),
            &String::from(vs_currency),
        ) {
            Some(&price) => Ok(price),
            None => Err(anyhow::anyhow!(
                "Price not found for token: {}",
                token_address
            )),
        }
    }

    async fn get_token_price_by_id(self: &Self, token_id: String) -> anyhow::Result<f64> {
        let vs_currency = "usd";
        let token_price_url = self
            .base_url
            .join(
                format!(
                    "/api/v3/simple/price?ids={}&vs_currencies={}",
                    token_id, vs_currency
                )
                .as_str(),
            )
            .expect("failed to join URL path");

        let mut builder = self.client.get(token_price_url);

        if let Some(x) = &self.api_key {
            builder = builder.header(CG_AUTH_HEADER, x)
        }

        let response = builder
            .send()
            .await?
            .json::<CoinGeckoPriceResponse>()
            .await?;
        match response.get_price(&token_id, &String::from(vs_currency)) {
            Some(&price) => Ok(price),
            None => Err(anyhow::anyhow!("Price not found for token: {}", token_id)),
        }
    }

    pub fn new(base_url: Url, api_key: Option<String>, client: reqwest::Client) -> Self {
        Self {
            base_url,
            api_key,
            client,
        }
    }
}

#[async_trait]
impl PriceAPIClient for CoinGeckoPriceAPIClient {
    async fn fetch_price(&self, token_address: Address) -> anyhow::Result<BaseTokenAPIPrice> {
        let token_usd_price = self.get_token_price_by_address(token_address).await?;
        let eth_usd_price = self.get_token_price_by_id(String::from("ethereum")).await?;
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
    use std::str::FromStr;

    use bigdecimal::BigDecimal;
    use chrono::Utc;
    use httpmock::MockServer;
    use url::Url;
    use zksync_types::{base_token_price::BaseTokenAPIPrice, Address};

    use crate::{
        coingecko_api::{CoinGeckoPriceAPIClient, CG_AUTH_HEADER},
        PriceAPIClient,
    };

    const TIME_TOLERANCE_MS: i64 = 100;

    fn add_mock_by_id(server: &MockServer, id: String, price: f64, api_key: Option<String>) {
        server.mock(|mut when, then| {
            when = when
                .method(httpmock::Method::GET)
                .path("/api/v3/simple/price");
            if let Some(x) = api_key {
                when = when.header(CG_AUTH_HEADER, x);
            }
            when.method(httpmock::Method::GET)
                .query_param("ids", &id)
                .query_param("vs_currencies", "usd");
            then.status(200)
                .body(format!("{{\"{}\":{{\"usd\":{}}}}}", &id, price));
        });
    }

    fn add_mock_by_address(
        server: &MockServer,
        // use string explicitly to verify that conversion of the address to string works as expected
        address: String,
        price: f64,
        api_key: Option<String>,
    ) {
        server.mock(|mut when, then| {
            when = when
                .method(httpmock::Method::GET)
                .path("/api/v3/simple/token_price/ethereum");
            if let Some(x) = api_key {
                when = when.header(CG_AUTH_HEADER, x);
            }
            when.query_param("contract_addresses", &address)
                .query_param("vs_currencies", "usd");
            then.status(200)
                .body(format!("{{\"{}\":{{\"usd\":{}}}}}", address, price));
        });
    }

    fn server_url(server: &MockServer) -> Url {
        Url::from_str(server.url("").as_str()).unwrap()
    }

    async fn test_happy_day(api_key: Option<String>) {
        let server = MockServer::start();
        let address = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
        let base_token_price = 198.9;
        let eth_price = 3000.0;
        add_mock_by_address(
            &server,
            address.to_string(),
            base_token_price,
            api_key.clone(),
        );
        add_mock_by_id(
            &server,
            String::from("ethereum"),
            eth_price,
            api_key.clone(),
        );

        let cg_client = CoinGeckoPriceAPIClient::new(
            server_url(&server),
            api_key.clone(),
            reqwest::Client::new(),
        );
        let api_price = cg_client
            .fetch_price(Address::from_str(address).unwrap())
            .await
            .unwrap();

        assert_eq!(
            BaseTokenAPIPrice {
                base_token_price: BigDecimal::from_str(&base_token_price.to_string()).unwrap(),
                eth_price: BigDecimal::from_str(&eth_price.to_string()).unwrap(),
                ratio_timestamp: api_price.ratio_timestamp,
            },
            api_price
        );
        assert!((Utc::now() - api_price.ratio_timestamp).num_milliseconds() <= TIME_TOLERANCE_MS);
    }

    #[tokio::test]
    async fn test_happy_day_with_api_key() {
        test_happy_day(Some("test".to_string())).await
    }

    #[tokio::test]
    async fn test_happy_day_no_api_key() {
        test_happy_day(None).await
    }

    #[tokio::test]
    async fn test_no_eth_price() {
        let server = MockServer::start();
        let address = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
        add_mock_by_address(&server, address.to_string(), 198.9, None);

        let cg_client =
            CoinGeckoPriceAPIClient::new(server_url(&server), None, reqwest::Client::new());
        let api_price = cg_client
            .fetch_price(Address::from_str(address).unwrap())
            .await;

        assert!(api_price.is_err());
    }

    #[tokio::test]
    async fn test_no_base_token_price() {
        let server = MockServer::start();
        let address = Address::from_str("0x1f9840a85d5af5bf1d1762f925bdaddc4201f984").unwrap();
        add_mock_by_id(&server, String::from("ethereum"), 29.5, None);

        let cg_client =
            CoinGeckoPriceAPIClient::new(server_url(&server), None, reqwest::Client::new());
        let api_price = cg_client.fetch_price(address).await;

        assert!(api_price.is_err());
    }
}
