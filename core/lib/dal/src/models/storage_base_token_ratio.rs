use std::num::NonZeroU64;

use bigdecimal::{BigDecimal, ToPrimitive};
use chrono::NaiveDateTime;
use zksync_types::base_token_ratio::BaseTokenRatio;

/// Represents a row in the `base_token_ratios` table.
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
            id: row.id as u32,
            ratio_timestamp: row.ratio_timestamp.and_utc(),
            numerator: NonZeroU64::new(row.numerator.to_u64().expect("numerator is not u64"))
                .unwrap(),
            denominator: NonZeroU64::new(row.denominator.to_u64().expect("denominator is not u64"))
                .unwrap(),
            used_in_l1: row.used_in_l1,
        }
    }
}
