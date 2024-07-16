use std::{collections::HashMap, str::FromStr};

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
    forced_ethereum_erc20_address: Option<Address>,
}

const PRO_COINGECKO_API_URL: &str = "https://pro-api.coingecko.com";
const DEMO_COINGECKO_API_URL: &str = "https://api.coingecko.com";
const PRO_COINGECKO_AUTH_HEADER: &str = "x-cg-pro-api-key";
const DEMO_AUTH_HEADER: &str = "x-cg-demo-api-key";
const ETH_ID: &str = "eth";

impl CoinGeckoPriceAPIClient {
    pub fn new(config: ExternalPriceApiClientConfig) -> Self {
        let base_url = config
            .base_url
            .clone()
            .unwrap_or(PRO_COINGECKO_API_URL.to_string());
        let client = if let Some(api_key) = &config.api_key {
            let header = match base_url.as_str() {
                PRO_COINGECKO_API_URL => PRO_COINGECKO_AUTH_HEADER,
                DEMO_COINGECKO_API_URL => DEMO_AUTH_HEADER,
                _ => PRO_COINGECKO_API_URL,
            };

            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert(
                reqwest::header::HeaderName::from_static(header),
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

        let erc20 = config.forced_ethereum_erc20_address;
        let forced_ethereum_erc20_address: Option<Address> = match erc20 {
            Some(erc20) => match Address::from_str(erc20.as_str()) {
                Ok(address) => Some(address),
                Err(_) => None,
            },
            None => None,
        };

        Self {
            base_url: Url::parse(&base_url).expect("Failed to parse CoinGecko URL"),
            client,
            forced_ethereum_erc20_address,
        }
    }

    async fn get_token_price_by_address(&self, address: Address) -> anyhow::Result<f64> {
        let address_str: String = match self.forced_ethereum_erc20_address {
            Some(erc20) => address_to_string(&erc20),
            None => address_to_string(&address),
        };
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
mod test {
    use std::str::FromStr;

    use zksync_config::configs::ExternalPriceApiClientConfig;
    use zksync_types::Address;

    use super::CoinGeckoPriceAPIClient;

    #[tokio::test]
    // This is not a proper test, as it requires user input and prints information.
    // It serves as a quick check to verify if the CoinGecko api_key is working correctly.
    async fn test_coingecko_api() {
        let config = ExternalPriceApiClientConfig {
            source: "coingecko".to_string(),
            base_url: Some("https://api.coingecko.com".to_string()),
            api_key: Some("insert_your_api".to_string()),
            client_timeout_ms: 30000,
            forced_numerator: Some(100),
            forced_denominator: Some(1),
            // If the below address is set the `get_token_price_by_address` will ommit the address passed as parameter
            //forced_ethereum_erc20_address: Some("0xdac17f958d2ee523a2206206994597c13d831ec7".to_string()),
            forced_ethereum_erc20_address: None,
        };
        let api = CoinGeckoPriceAPIClient::new(config);

        println!(
            "{:?}",
            api.get_token_price_by_address(
                Address::from_str("0x1f9840a85d5af5bf1d1762f925bdaddc4201f984").unwrap()
            )
            .await
            .unwrap()
        );
    }
}
