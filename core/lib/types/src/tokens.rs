use serde::{Deserialize, Serialize};
use zksync_basic_types::Address;
pub use zksync_system_constants::ETHEREUM_ADDRESS;

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TokenInfo {
    pub l1_address: Address,
    pub l2_address: Address,
    pub metadata: TokenMetadata,
}

/// Relevant information about tokens supported by ZKsync protocol.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TokenMetadata {
    /// Token name (e.g. "Ethereum" or "USD Coin")
    pub name: String,
    /// Token symbol (e.g. "ETH" or "USDC")
    pub symbol: String,
    /// Token precision (e.g. 18 for "ETH" so "1.0" ETH = 10e18 as U256 number)
    pub decimals: u8,
}

impl TokenMetadata {
    /// Creates a default representation of token data, which will be used for tokens that have
    /// not known metadata.
    pub fn default(address: Address) -> Self {
        let default_name = format!("ERC20-{:x}", address);
        Self {
            name: default_name.clone(),
            symbol: default_name,
            decimals: 18,
        }
    }
}
