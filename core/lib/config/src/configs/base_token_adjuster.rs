use std::time::Duration;

use serde::Deserialize;

/// By default, the ratio persister will run every 30 seconds.
pub const DEFAULT_INTERVAL_MS: u64 = 30_000;

/// By default, refetch ratio from db every 0.5 second
pub const DEFAULT_CACHE_UPDATE_INTERVAL: u64 = 500;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BaseTokenAdjusterConfig {
    /// How often to spark a new cycle of the ratio persister to fetch external prices and persis ratios.
    #[serde(default = "BaseTokenAdjusterConfig::default_pooling_interval")]
    pub price_polling_interval_ms: u64,

    /// We (in memory) cache the ratio fetched from db. This interval defines frequency of refetch from db.
    #[serde(default = "BaseTokenAdjusterConfig::default_cache_update_interval")]
    pub price_cache_update_interval_ms: u64,
}

impl Default for BaseTokenAdjusterConfig {
    fn default() -> Self {
        Self {
            price_polling_interval_ms: Self::default_pooling_interval(),
            price_cache_update_interval_ms: Self::default_cache_update_interval(),
        }
    }
}

impl BaseTokenAdjusterConfig {
    fn default_pooling_interval() -> u64 {
        DEFAULT_INTERVAL_MS
    }

    pub fn price_polling_interval(&self) -> Duration {
        Duration::from_millis(self.price_polling_interval_ms)
    }

    fn default_cache_update_interval() -> u64 {
        DEFAULT_CACHE_UPDATE_INTERVAL
    }

    pub fn price_cache_update_interval(&self) -> Duration {
        Duration::from_millis(self.price_cache_update_interval_ms)
    }
}
