use bigdecimal::{BigDecimal, ToPrimitive};
use chrono::NaiveDateTime;
use zksync_types::base_token_price::BaseTokenRatio;

/// Represents a row in the `storage_base_token_price` table.
#[derive(Debug, Clone)]
pub struct StorageBaseTokenRatio {
    pub id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub ratio_timestamp: NaiveDateTime,
    pub numerator: BigDecimal,
    pub denominator: BigDecimal,
    pub used_in_l1: bool,
}

impl From<StorageBaseTokenRatio> for BaseTokenRatio {
    fn from(row: StorageBaseTokenRatio) -> BaseTokenRatio {
        BaseTokenRatio {
            id: row.id,
            ratio_timestamp: row.ratio_timestamp.and_utc(),
            numerator: row.numerator.to_u64().expect("numerator is not u64"),
            denominator: row.denominator.to_u64().expect("denominator is not u64"),
            used_in_l1: row.used_in_l1,
        }
    }
}
