use std::{collections::HashMap, str::FromStr};

use async_trait::async_trait;
use chrono::Utc;
use itertools::Itertools;
use num::{rational::Ratio, BigUint};
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};

use zksync_config::FetcherConfig;
use zksync_types::{tokens::TokenMarketVolume, Address};
use zksync_utils::UnsignedRatioSerializeAsDecimal;

use crate::data_fetchers::error::ApiFetchError;

use super::FetcherImpl;

#[derive(Debug, Clone)]
pub struct UniswapTradingVolumeFetcher {
    client: Client,
    addr: Url,
}

impl UniswapTradingVolumeFetcher {
    pub fn new(config: &FetcherConfig) -> Self {
        Self {
            client: Client::new(),
            addr: Url::from_str(&config.token_trading_volume.url)
                .expect("failed parse Uniswap URL"),
        }
    }
}

#[async_trait]
impl FetcherImpl for UniswapTradingVolumeFetcher {
    async fn fetch_trading_volumes(
        &self,
        tokens: &[Address],
    ) -> Result<HashMap<Address, TokenMarketVolume>, ApiFetchError> {
        let comma_separated_token_addresses = tokens
            .iter()
            .map(|token_addr| format!("\"{:#x}\"", token_addr))
            .join(",");

        let query = format!(
            "{{tokens(where:{{id_in:[{}]}}){{id, untrackedVolumeUSD}}}}",
            comma_separated_token_addresses
        );

        let last_updated = Utc::now();

        let raw_response = self
            .client
            .post(self.addr.clone())
            .json(&serde_json::json!({
                "query": query,
            }))
            .send()
            .await
            .map_err(|err| {
                ApiFetchError::ApiUnavailable(format!("Uniswap API request failed: {}", err))
            })?;

        let response_status = raw_response.status();
        let response_text = raw_response.text().await.map_err(|err| {
            ApiFetchError::Other(format!(
                "Error: {} while while decoding response to text",
                err
            ))
        })?;

        let response: GraphqlResponse = serde_json::from_str(&response_text).map_err(|err| {
            ApiFetchError::UnexpectedJsonFormat(format!(
                "Error: {} while decoding response: {} with status: {}",
                err, response_text, response_status
            ))
        })?;

        let result = response
            .data
            .tokens
            .into_iter()
            .map(|token_response| {
                (
                    token_response.id,
                    TokenMarketVolume {
                        market_volume: token_response.untracked_volume_usd,
                        last_updated,
                    },
                )
            })
            .collect();

        Ok(result)
    }
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GraphqlResponse {
    pub data: GraphqlTokensResponse,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct GraphqlTokensResponse {
    pub tokens: Vec<TokenResponse>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TokenResponse {
    pub id: Address,
    /// Total amount swapped all time in token pair stored in USD, no minimum liquidity threshold.
    #[serde(
        with = "UnsignedRatioSerializeAsDecimal",
        rename = "untrackedVolumeUSD"
    )]
    pub untracked_volume_usd: Ratio<BigUint>,
}

#[tokio::test]
#[ignore] // Remote API may be unavailable, so we ignore this test by default.
async fn test_fetch_uniswap_trading_volumes() {
    let mut config = FetcherConfig::from_env();
    config.token_trading_volume.url =
        "https://api.thegraph.com/subgraphs/name/uniswap/uniswap-v2".to_string();

    let fetcher = UniswapTradingVolumeFetcher::new(&config);

    let tokens = vec![
        Address::from_str("6b175474e89094c44da98b954eedeac495271d0f").expect("DAI"),
        Address::from_str("1f9840a85d5af5bf1d1762f925bdaddc4201f984").expect("UNI"),
        Address::from_str("514910771af9ca656af840dff83e8264ecf986ca").expect("LINK"),
    ];

    let token_volumes = fetcher
        .fetch_trading_volumes(&tokens)
        .await
        .expect("failed get tokens price");

    assert_eq!(
        token_volumes.len(),
        tokens.len(),
        "not all data was received"
    );
    for token_address in tokens {
        assert!(token_volumes.get(&token_address).is_some());
    }
}
