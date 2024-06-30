use std::num::NonZeroU64;

use chrono::{DateTime, Utc};

/// Represents the base token to ETH conversion ratio at a given point in time.
#[derive(Debug, Clone)]
pub struct BaseTokenRatio {
    pub id: i64,
    pub ratio_timestamp: DateTime<Utc>,
    pub numerator: NonZeroU64,
    pub denominator: NonZeroU64,
    pub used_in_l1: bool,
}

/// Struct to represent API response containing denominator, numerator, and timestamp.
#[derive(Debug)]
pub struct BaseTokenAPIRatio {
    pub numerator: NonZeroU64,
    pub denominator: NonZeroU64,
    // Either the timestamp of the quote or the timestamp of the request.
    pub ratio_timestamp: DateTime<Utc>,
}
