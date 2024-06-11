use std::time::Duration;

use serde::Deserialize;

pub const DEFAULT_INTERVAL_MS: u32 = 5000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BaseTokenAdjusterConfig {
    /// How often to poll external APIs for a new ETH<->BaseToken price.
    pub price_polling_interval_ms: u32,
}

impl BaseTokenAdjusterConfig {
    pub fn for_tests() -> Self {
        Self {
            price_polling_interval_ms: Some(DEFAULT_INTERVAL_MS),
        }
    }

    pub fn price_polling_interval(&self) -> Duration {
        match &self.price_polling_interval_ms {
            Some(interval) => Duration::from_millis(interval as u64),
            None => Duration::from_millis(DEFAULT_INTERVAL_MS as u64),
        }
    }
}
