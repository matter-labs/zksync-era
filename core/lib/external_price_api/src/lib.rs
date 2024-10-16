pub mod cmc_api;
pub mod coingecko_api;
pub mod forced_price_client;
#[cfg(test)]
mod tests;
mod utils;

use std::fmt;

use async_trait::async_trait;
use zksync_types::{base_token_ratio::BaseTokenAPIRatio, Address};

/// Trait that defines the interface for a client connecting with an external API to get prices.
#[async_trait]
pub trait PriceAPIClient: Sync + Send + fmt::Debug + 'static {
    /// Returns the BaseToken<->ETH ratio for the input token address.
    /// The returned value is rational number X such that X BaseToken = 1 ETH.
    /// Example if 1 BaseToken = 0.002 ETH, then ratio is 500/1 (500 BaseToken = 1ETH)
    async fn fetch_ratio(&self, token_address: Address) -> anyhow::Result<BaseTokenAPIRatio>;
}

// Struct for a no-op PriceAPIClient (conversion ratio is always 1:1).
#[derive(Debug, Clone)]
pub struct NoOpPriceAPIClient;

#[async_trait]
impl PriceAPIClient for NoOpPriceAPIClient {
    async fn fetch_ratio(&self, _token_address: Address) -> anyhow::Result<BaseTokenAPIRatio> {
        Ok(BaseTokenAPIRatio::default())
    }
}

fn address_to_string(address: &Address) -> String {
    format!("{:#x}", address)
}
