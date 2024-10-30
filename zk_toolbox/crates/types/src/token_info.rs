use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct TokenInfo {
    pub name: String,
    pub symbol: String,
    pub decimals: u8,
}

impl TokenInfo {
    pub fn eth() -> Self {
        Self {
            name: "Ether".to_string(),
            symbol: "ETH".to_string(),
            decimals: 18,
        }
    }
}
