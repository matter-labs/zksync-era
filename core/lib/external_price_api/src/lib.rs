pub mod cmc_api;
pub mod coingecko_api;
pub mod forced_price_client;
#[cfg(feature = "node_framework")]
pub mod node;
#[cfg(test)]
mod tests;
mod utils;

use std::fmt;

use async_trait::async_trait;
use zksync_types::{base_token_ratio::BaseTokenApiRatio, Address, ZK_L1_ADDRESS};

/// Enum representing the token for which the ratio is fetched.
#[derive(Debug, Clone, Copy)]
pub enum APIToken {
    Eth,            // we return price in ETH right now so this will always return identity
    ERC20(Address), // on Ethereum
    ZK,             // ZK is not tracked by API sources by any Ethereum address (only by L2 address)
}

impl APIToken {
    pub fn from_config_address(address: Address) -> Self {
        if address == Address::zero() || address == Address::from_low_u64_be(1) {
            Self::Eth
            // ZK needs special handling due to being not recognized by API clients
        } else if address == ZK_L1_ADDRESS {
            Self::ZK
        } else {
            Self::ERC20(address)
        }
    }
}

/// Trait that defines the interface for a client connecting with an external API to get prices.
#[async_trait]
pub trait PriceApiClient: Sync + Send + fmt::Debug + 'static {
    /// Returns the BaseToken<->ETH ratio for the input token address.
    /// The returned value is rational number X such that X BaseToken = 1 ETH.
    /// Example if 1 BaseToken = 0.002 ETH, then ratio is 500/1 (500 BaseToken = 1ETH)
    async fn fetch_ratio(&self, token: APIToken) -> anyhow::Result<BaseTokenApiRatio>;
}

/// No-op [`PriceApiClient`] (conversion ratio is always 1:1).
#[derive(Debug, Clone)]
pub struct NoOpPriceApiClient;

#[async_trait]
impl PriceApiClient for NoOpPriceApiClient {
    async fn fetch_ratio(&self, _token: APIToken) -> anyhow::Result<BaseTokenApiRatio> {
        Ok(BaseTokenApiRatio::default())
    }
}

fn address_to_string(address: &Address) -> String {
    format!("{:#x}", address)
}

#[cfg(test)]
mod test {
    use assert_matches::assert_matches;

    use super::*;

    #[tokio::test]
    async fn test_base_token_from_config() {
        let eth_address1 = Address::zero();
        let eth_address2 = Address::from_low_u64_be(1);
        let custom_address = Address::from_low_u64_be(0x1234);

        // Test ETH address recognition
        assert_matches!(APIToken::from_config_address(eth_address1), APIToken::Eth);
        assert_matches!(APIToken::from_config_address(eth_address2), APIToken::Eth);
        assert_matches!(APIToken::from_config_address(custom_address), APIToken::ERC20(addr) if addr == custom_address);
    }
}
