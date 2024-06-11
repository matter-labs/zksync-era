use bigdecimal::BigDecimal;
use chrono::NaiveDateTime;
use zksync_types::base_token_price::BaseTokenPrice;

/// Represents a row in the `storage_base_token_price` table.
#[derive(Debug, Clone)]
pub struct StorageBaseTokenPrice {
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub ratio_timestamp: NaiveDateTime,
    pub numerator: BigDecimal,
    pub denominator: BigDecimal,
    pub used_in_l1: bool,
}

impl From<StorageBaseTokenPrice> for BaseTokenPrice {
    fn from(row: StorageBaseTokenPrice) -> BaseTokenPrice {
        BaseTokenPrice {
            ratio_timestamp: row.ratio_timestamp.and_utc(),
            numerator: row.numerator,
            denominator: row.denominator,
            used_in_l1: row.used_in_l1,
        }
    }
}
