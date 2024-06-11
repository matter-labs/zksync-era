use chrono::{DateTime, Utc};

/// Represents the base token price at a given point in time.
#[derive(Debug, Clone)]
pub struct BaseTokenPrice {
    pub ratio_timestamp: DateTime<Utc>,
    pub numerator: BigDecimal,
    pub denominator: BigDecimal,
    pub used_in_l1: bool,
}
