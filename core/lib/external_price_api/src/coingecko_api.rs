use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use reqwest;
use serde::{Deserialize, Serialize};
use url::Url;
use zksync_config::configs::ExternalPriceApiClientConfig;
use zksync_types::{base_token_ratio::BaseTokenApiRatio, Address};

use crate::{address_to_string, utils::get_fraction, PriceApiClient};

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
                .timeout(config.client_timeout_ms)
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

    /// returns token price in ETH by token address. Returned value is X such that 1 TOKEN = X ETH.
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
impl PriceApiClient for CoinGeckoPriceAPIClient {
    async fn fetch_ratio(&self, token_address: Address) -> anyhow::Result<BaseTokenApiRatio> {
        let base_token_in_eth = self.get_token_price_by_address(token_address).await?;
        let (num_in_eth, denom_in_eth) = get_fraction(base_token_in_eth)?;
        // take reciprocal of price as returned price is ETH/BaseToken and BaseToken/ETH is needed
        let (num_in_base, denom_in_base) = (denom_in_eth, num_in_eth);

        return Ok(BaseTokenApiRatio {
            numerator: num_in_base,
            denominator: denom_in_base,
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

#[cfg(test)]
mod test {
    use httpmock::MockServer;

    use super::*;
    use crate::tests::*;

    fn get_mock_response(address: &str, price: f64) -> String {
        format!("{{\"{}\":{{\"eth\":{}}}}}", address, price)
    }

    #[test]
    fn test_mock_response() {
        // curl "https://api.coingecko.com/api/v3/simple/token_price/ethereum?contract_addresses=0x1f9840a85d5af5bf1d1762f925bdaddc4201f984&vs_currencies=eth"
        // {"0x1f9840a85d5af5bf1d1762f925bdaddc4201f984":{"eth":0.00269512}}
        assert_eq!(
            get_mock_response("0x1f9840a85d5af5bf1d1762f925bdaddc4201f984", 0.00269512),
            r#"{"0x1f9840a85d5af5bf1d1762f925bdaddc4201f984":{"eth":0.00269512}}"#
        )
    }

    fn add_mock_by_address(
        server: &MockServer,
        // use string explicitly to verify that conversion of the address to string works as expected
        address: String,
        price: Option<f64>,
        api_key: Option<String>,
    ) {
        server.mock(|mut when, then| {
            when = when
                .method(httpmock::Method::GET)
                .path("/api/v3/simple/token_price/ethereum");

            when = when.query_param("contract_addresses", address.clone());
            when = when.query_param("vs_currencies", ETH_ID);
            api_key.map(|key| when.header(COINGECKO_AUTH_HEADER, key));

            if let Some(p) = price {
                then.status(200).body(get_mock_response(&address, p));
            } else {
                // requesting with invalid/unknown address results in empty json
                // example:
                // $ curl "https://api.coingecko.com/api/v3/simple/token_price/ethereum?contract_addresses=0x000000000000000000000000000000000000dead&vs_currencies=eth"
                // {}
                then.status(200).body("{}");
            };
        });
    }

    fn get_config(base_url: String, api_key: Option<String>) -> ExternalPriceApiClientConfig {
        ExternalPriceApiClientConfig {
            base_url: Some(base_url),
            api_key,
            source: "coingecko".to_string(),
            ..ExternalPriceApiClientConfig::default()
        }
    }

    fn happy_day_setup(
        api_key: Option<String>,
        server: &MockServer,
        address: Address,
        base_token_price: f64,
    ) -> SetupResult {
        add_mock_by_address(
            server,
            address_to_string(&address),
            Some(base_token_price),
            api_key.clone(),
        );
        SetupResult {
            client: Box::new(CoinGeckoPriceAPIClient::new(get_config(
                server.url(""),
                api_key,
            ))),
        }
    }

    #[tokio::test]
    async fn test_happy_day_with_api_key() {
        happy_day_test(
            |server: &MockServer, address: Address, base_token_price: f64| {
                happy_day_setup(
                    Some("test-key".to_string()),
                    server,
                    address,
                    base_token_price,
                )
            },
        )
        .await
    }

    #[tokio::test]
    async fn test_happy_day_with_no_api_key() {
        happy_day_test(
            |server: &MockServer, address: Address, base_token_price: f64| {
                happy_day_setup(None, server, address, base_token_price)
            },
        )
        .await
    }

    fn error_404_setup(
        server: &MockServer,
        _address: Address,
        _base_token_price: f64,
    ) -> SetupResult {
        // just don't add mock
        SetupResult {
            client: Box::new(CoinGeckoPriceAPIClient::new(get_config(
                server.url(""),
                Some("FILLER".to_string()),
            ))),
        }
    }

    #[tokio::test]
    async fn test_error_404() {
        let error_string = error_test(error_404_setup).await.to_string();
        assert!(
            error_string
                .starts_with("Http error while fetching token price. Status: 404 Not Found"),
            "Error was: {}",
            &error_string
        )
    }

    fn error_missing_setup(
        server: &MockServer,
        address: Address,
        _base_token_price: f64,
    ) -> SetupResult {
        let api_key = Some("FILLER".to_string());

        add_mock_by_address(server, address_to_string(&address), None, api_key.clone());
        SetupResult {
            client: Box::new(CoinGeckoPriceAPIClient::new(get_config(
                server.url(""),
                api_key,
            ))),
        }
    }

    #[tokio::test]
    async fn test_error_missing() {
        let error_string = error_test(error_missing_setup).await.to_string();
        assert!(
            error_string.starts_with("Price not found for token"),
            "Error was: {}",
            error_string
        )
    }
}
