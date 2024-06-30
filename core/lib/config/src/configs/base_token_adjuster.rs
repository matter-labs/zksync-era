use std::{num::NonZeroU64, time::Duration};

use serde::Deserialize;

// By default external APIs shall be polled every 30 seconds for a new price.
pub const DEFAULT_INTERVAL_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BaseTokenAdjusterConfig {
    /// How often to fetch external APIs for a new ETH<->Base-Token price.
    pub price_polling_interval_ms: Option<u64>,
    /// The numerator and the denominator make up the BaseToken/ETH conversion ratio. Upon initialization,
    /// the ratio is set to these configured values to avoid loss of liveness while we wait for external APIs.
    pub initial_numerator: NonZeroU64,
    pub initial_denominator: NonZeroU64,
}

impl BaseTokenAdjusterConfig {
    pub fn price_polling_interval(&self) -> Duration {
        match self.price_polling_interval_ms {
            Some(interval) => Duration::from_millis(interval),
            None => Duration::from_millis(DEFAULT_INTERVAL_MS),
        }
    }
}
