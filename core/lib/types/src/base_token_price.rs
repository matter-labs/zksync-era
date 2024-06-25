use chrono::{DateTime, Utc};

/// Represents the base token to ETH ratio at a given point in time.
#[derive(Debug, Clone)]
pub struct BaseTokenRatio {
    pub id: i64,
    pub ratio_timestamp: DateTime<Utc>,
    pub numerator: u64,
    pub denominator: u64,
    pub used_in_l1: bool,
}

/// Struct to represent API response containing denominator, numerator and optional timestamp.
#[derive(Debug)]
pub struct BaseTokenAPIRatio {
    pub numerator: u64,
    pub denominator: u64,
    // Either the timestamp of the quote or the timestamp of the request.
    pub ratio_timestamp: DateTime<Utc>,
}
