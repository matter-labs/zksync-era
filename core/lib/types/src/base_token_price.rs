use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};

/// Represents the base token price at a given point in time.
#[derive(Debug, Clone, Default)]
pub struct BaseTokenPrice {
    pub id: i64,
    pub ratio_timestamp: DateTime<Utc>,
    pub base_token_price: BigDecimal,
    pub eth_price: BigDecimal,
    pub used_in_l1: bool,
}

/// Struct to represent API response containing denominator, numerator and optional timestamp.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct BaseTokenAPIPrice {
    pub base_token_price: BigDecimal,
    pub eth_price: BigDecimal,
    // Either the timestamp of the quote or the timestamp of the request.
    pub ratio_timestamp: DateTime<Utc>,
}
