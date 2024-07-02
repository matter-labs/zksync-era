use serde::Deserialize;

// By default external APIs shall be polled every 30 seconds for a new price.
pub const DEFAULT_INTERVAL_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BaseTokenAdjusterConfig {
    /// How often to fetch external APIs for a new ETH<->Base-Token price.
    #[serde(default = "BaseTokenAdjusterConfig::default_interval")]
    pub price_polling_interval_ms: u64,
}

impl Default for BaseTokenAdjusterConfig {
    fn default() -> Self {
        Self {
            price_polling_interval_ms: Self::default_interval(),
        }
    }
}

impl BaseTokenAdjusterConfig {
    pub fn default_interval() -> u64 {
        DEFAULT_INTERVAL_MS
    }
}
