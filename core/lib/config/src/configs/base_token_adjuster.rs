use std::time::Duration;

use serde::Deserialize;

// Fetch new prices every 30 seconds by default.
pub const DEFAULT_INTERVAL_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BaseTokenAdjusterConfig {
    /// How often to poll external APIs for a new ETH<->Base-Token price.
    pub price_polling_interval_ms: Option<u64>,
}

impl BaseTokenAdjusterConfig {
    pub fn for_tests() -> Self {
        Self {
            price_polling_interval_ms: Some(DEFAULT_INTERVAL_MS),
        }
    }

    pub fn price_polling_interval(&self) -> Duration {
        match self.price_polling_interval_ms {
            Some(interval) => Duration::from_millis(interval),
            None => Duration::from_millis(DEFAULT_INTERVAL_MS),
        }
    }
}
