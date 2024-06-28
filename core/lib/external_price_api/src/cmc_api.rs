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
    api_key: Option<String>,
    client: reqwest::Client,
    token_id_by_address: HashMap<Address, i32>,
}

impl CMCPriceAPIClient {
    fn new(base_url: Url, api_key: Option<String>, client: reqwest::Client) -> Self {
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
                let address_str = address_to_string(address);
                let mut builder = self.client.get(url);
                if let Some(x) = &self.api_key {
                    builder = builder.header(CMC_AUTH_HEADER, x)
                }

                let response = builder.send().await?;
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

        let mut builder = self.client.get(price_url);

        if let Some(x) = &self.api_key {
            builder = builder.header(CMC_AUTH_HEADER, x);
        }

        let response = builder.send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Http error while fetching token price by address. Status: {}, token: {}, msg: {}",
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

mod tests {
    use std::str::FromStr;

    use url::Url;
    use zksync_types::Address;

    use crate::{cmc_api::CMCPriceAPIClient, PriceAPIClient};

    #[tokio::test]
    async fn test_happy_day() {
        let address = "0x1f9840a85d5af5bf1d1762f925bdaddc4201f984";
        let api_key = "".to_string();

        let mut cmc_client = CMCPriceAPIClient::new(
            Url::from_str("https://pro-api.coinmarketcap.com/").unwrap(),
            Some(api_key.clone()),
            reqwest::Client::new(),
        );
        let api_price = cmc_client
            .fetch_price(Address::from_str(address).unwrap())
            .await
            .unwrap();
        print!("{:?}", api_price);
        assert_eq!(0.0, 1.0);
    }
}
