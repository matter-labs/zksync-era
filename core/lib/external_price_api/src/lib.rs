use std::fmt;

use async_trait::async_trait;
use zksync_types::Address;

/// Trait that defines the interface for a client connecting with an external API to get prices.
#[async_trait]
pub trait PriceAPIClient: Sync + Send + fmt::Debug {
    /// Returns the price for the input token address in dollars.
    async fn fetch_price(&self, token_address: Address) -> Result<f64, APIError>;
}
