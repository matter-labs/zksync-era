pub mod coingecko_api;
mod tests;

use std::fmt;

use async_trait::async_trait;
use zksync_types::{base_token_ratio::BaseTokenAPIPrice, Address};

/// Trait that defines the interface for a client connecting with an external API to get prices.
#[async_trait]
pub trait PriceAPIClient: Sync + Send + fmt::Debug {
    /// Returns the price for the input token address in $USD.
    async fn fetch_prices(&self, token_address: Address) -> anyhow::Result<BaseTokenAPIPrice>;
}

fn address_to_string(address: &Address) -> String {
    format!("{:#x}", address)
}
