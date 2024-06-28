use std::fmt::Debug;

use async_trait::async_trait;
use zksync_types::{base_token_price::BaseTokenAPIPrice, Address};

mod cmc_api;
mod coingecko_api;
mod tests;

/// Trait that defines the interface for a client connecting with an external API to get prices.
#[async_trait]
pub trait PriceAPIClient: Sync + Send + Debug {
    /// Returns the price for the input token address in $USD.
    async fn fetch_price(&mut self, token_address: Address) -> anyhow::Result<BaseTokenAPIPrice>;
}

fn address_to_string(address: &Address) -> String {
    format!("{:#x}", address)
}
