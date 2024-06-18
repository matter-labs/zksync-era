use serde::Deserialize;
use zksync_basic_types::Address;

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct BaseTokenConfig {
    pub base_token_address: Address,
    pub outdated_token_price_timeout: Option<u64>,
}

impl Default for BaseTokenConfig {
    fn default() -> Self {
        Self {
            base_token_address: Address::default(),
            outdated_token_price_timeout: None,
        }
    }
}
