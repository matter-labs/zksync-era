use std::collections::HashMap;

use async_trait::async_trait;
use bigdecimal::FromPrimitive;
use chrono::Utc;
use num::{rational::Ratio, BigUint};
use zksync_types::{tokens::TokenMarketVolume, Address};

use crate::data_fetchers::error::ApiFetchError;

use super::FetcherImpl;

#[derive(Debug, Clone)]
pub struct MockTradingVolumeFetcher {}

impl MockTradingVolumeFetcher {
    pub fn new() -> Self {
        Self {}
    }

    pub fn volume(&self, _token: &Address) -> TokenMarketVolume {
        TokenMarketVolume {
            market_volume: Ratio::from(BigUint::from_u64(1).unwrap()), // We don't use volume in the server anymore.
            last_updated: Utc::now(),
        }
    }
}

#[async_trait]
impl FetcherImpl for MockTradingVolumeFetcher {
    async fn fetch_trading_volumes(
        &self,
        tokens: &[Address],
    ) -> Result<HashMap<Address, TokenMarketVolume>, ApiFetchError> {
        let volumes: HashMap<_, _> = tokens
            .iter()
            .map(|token| (*token, self.volume(token)))
            .collect();
        Ok(volumes)
    }
}
