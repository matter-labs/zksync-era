use serde::Deserialize;

/// By default the ratio persister will run every 30 seconds.
pub const DEFAULT_INTERVAL_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BaseTokenAdjusterConfig {
    /// How often to spark a new cycle of the ratio persister to fetch external prices and persis ratios.
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
    fn default_interval() -> u64 {
        DEFAULT_INTERVAL_MS
    }
}
