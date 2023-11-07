use std::{collections::HashMap, str::FromStr};

use async_trait::async_trait;
use reqwest::{Client, Url};
use serde::{Deserialize, Serialize};

use zksync_config::FetcherConfig;
use zksync_types::{tokens::TokenMetadata, Address};

use crate::data_fetchers::error::ApiFetchError;

use super::FetcherImpl;

#[derive(Debug, Clone)]
pub struct OneInchTokenListFetcher {
    client: Client,
    addr: Url,
}

impl OneInchTokenListFetcher {
    pub fn new(config: &FetcherConfig) -> Self {
        Self {
            client: Client::new(),
            addr: Url::from_str(&config.token_list.url).expect("failed parse One Inch URL"),
        }
    }
}

#[async_trait]
impl FetcherImpl for OneInchTokenListFetcher {
    async fn fetch_token_list(&self) -> Result<HashMap<Address, TokenMetadata>, ApiFetchError> {
        let token_list_url = self
            .addr
            .join("/v3.0/1/tokens")
            .expect("failed to join URL path");

        let token_list = self
            .client
            .get(token_list_url.clone())
            .send()
            .await
            .map_err(|err| {
                ApiFetchError::ApiUnavailable(format!("{} , Error: {}", token_list_url, err))
            })?
            .json::<OneInchTokensResponse>()
            .await
            .map_err(|err| ApiFetchError::UnexpectedJsonFormat(err.to_string()))?
            .tokens;

        Ok(token_list)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct OneInchTokensResponse {
    pub tokens: HashMap<Address, TokenMetadata>,
}
