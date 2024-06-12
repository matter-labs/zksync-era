use chrono::{DateTime, Utc};
use bigdecimal::BigDecimal;

/// Represents the base token price at a given point in time.
#[derive(Debug, Clone)]
pub struct BaseTokenPrice {
    pub id: i64,
    pub ratio_timestamp: DateTime<Utc>,
    pub numerator: BigDecimal,
    pub denominator: BigDecimal,
    pub used_in_l1: bool,
}

/// Struct to represent API response containing denominator, numerator and optional timestamp.
#[derive(Debug)]
pub struct BaseTokenAPIPrice {
    pub numerator: BigDecimal,
    pub denominator: BigDecimal,
    // Either the timestamp of the quote or the timestamp of the request.
    pub ratio_timestamp: DateTime<Utc>,
}