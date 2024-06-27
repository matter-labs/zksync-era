use std::collections::HashMap;

use serde::Deserialize;
use url::Url;
use zksync_types::Address;

use crate::address_to_string;

const CMC_AUTH_HEADER: &str = "X-CMC_PRO_API_KEY";

#[derive(Debug)]
pub struct CoinMarketCapPriceAPIClient {
    base_url: Url,
    api_key: Option<String>,
    client: reqwest::Client,
    token_id_by_address: HashMap<Address, String>,
}

impl CoinMarketCapPriceAPIClient {
    async fn get_token_id(self: &Self, address: Address) -> anyhow::Result<String> {
        match self.token_id_by_address.get(&address) {
            Some(x) => Ok(x.clone()),
            None => {
                let url = self
                    .base_url
                    .join("/v1/cryptocurrency/map")
                    .expect("failed to join URL path");
                let mut builder = self.client.get(url);
                if let Some(x) = &self.api_key {
                    builder = builder.header(CMC_AUTH_HEADER, x)
                }
                let response = builder.send().await?.json::<CMCMapResponse>().await?;

                let address_str = address_to_string(address);
                for crypto in response.data {
                    if let Some(platform) = crypto.platform {
                        if platform.token_address == address_str {
                            return Ok(crypto.id.to_string());
                        }
                    }
                }

                Err(anyhow::anyhow!("Token ID not found for the given address"))
            }
        }
    }

    async fn get_token_price_by_address(
        self: &Self,
        token_address: Address,
    ) -> anyhow::Result<f64> {
        let token_id = self.get_token_id(token_address).await?;

        let token_price_url = self
            .base_url
            .join(format!("/v1/cryptocurrency/quotes/latest?id={}", &token_id).as_str())
            .expect("failed to join URL path");

        let mut builder = self.client.get(token_price_url);

        if let Some(x) = &self.api_key {
            builder = builder.header(CMC_AUTH_HEADER, x);
        }

        let response = builder.send().await?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to fetch data: {}",
                response.status()
            ));
        }

        let response_json = response.json::<serde_json::Value>().await?;

        let quote = response_json["data"][&token_id]["quote"]["USD"]["price"]
            .as_f64()
            .ok_or_else(|| anyhow::anyhow!("Price not found for token ID: {}", &token_id))?;

        Ok(quote)
    }
}

#[derive(Debug, Deserialize)]
struct CMCMapResponse {
    data: Vec<CMCCryptoInfo>,
    status: Status,
}

#[derive(Debug, Deserialize)]
struct CMCCryptoInfo {
    id: i32,
    name: String,
    symbol: String,
    slug: String,
    platform: Option<CMCCryptoPlatform>,
}

#[derive(Debug, Deserialize)]
struct CMCCryptoPlatform {
    id: i32,
    name: String,
    symbol: String,
    slug: String,
    token_address: String,
}

#[derive(Debug, Deserialize)]
struct Status {
    timestamp: String,
    error_code: i32,
    error_message: Option<String>,
    elapsed: i32,
    credit_count: i32,
}

mod tests {}
