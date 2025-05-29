use std::num::NonZeroU64;

use chrono::{DateTime, Utc};

/// Represents the base token to ETH conversion ratio at a given point in time.
#[derive(Debug, Clone)]
pub struct BaseTokenRatio {
    pub id: u32,
    pub ratio_timestamp: DateTime<Utc>,
    pub numerator: NonZeroU64,
    pub denominator: NonZeroU64,
    pub used_in_l1: bool,
}

/// Struct to represent API response containing denominator, numerator, and timestamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BaseTokenApiRatio {
    pub numerator: NonZeroU64,
    pub denominator: NonZeroU64,
    // Either the timestamp of the quote or the timestamp of the request.
    pub ratio_timestamp: DateTime<Utc>,
}

impl Default for BaseTokenApiRatio {
    fn default() -> Self {
        Self {
            numerator: NonZeroU64::new(1).unwrap(),
            denominator: NonZeroU64::new(1).unwrap(),
            ratio_timestamp: Utc::now(),
        }
    }
}
