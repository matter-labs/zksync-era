use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use num::{rational::Ratio, BigUint};
use zksync_types::{
    tokens::{TokenPrice, ETHEREUM_ADDRESS},
    Address,
};

use crate::data_fetchers::error::ApiFetchError;

use super::FetcherImpl;

#[derive(Debug, Default, Clone)]
pub struct MockPriceFetcher;

impl MockPriceFetcher {
    pub fn new() -> Self {
        Self
    }

    pub fn token_price(&self, token: &Address) -> TokenPrice {
        let raw_base_price = if *token == ETHEREUM_ADDRESS {
            1500u64
        } else {
            1u64
        };
        let usd_price = Ratio::from_integer(BigUint::from(raw_base_price));

        TokenPrice {
            usd_price,
            last_updated: Utc::now(),
        }
    }
}

#[async_trait]
impl FetcherImpl for MockPriceFetcher {
    async fn fetch_token_price(
        &self,
        tokens: &[Address],
    ) -> Result<HashMap<Address, TokenPrice>, ApiFetchError> {
        let data: HashMap<_, _> = tokens
            .iter()
            .map(|token| (*token, self.token_price(token)))
            .collect();
        Ok(data)
    }
}
