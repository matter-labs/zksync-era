use std::num::NonZeroU64;

use bigdecimal::{BigDecimal, ToPrimitive};
use chrono::NaiveDateTime;
use zksync_types::{
    base_token_ratio::BaseTokenRatio,
    fee_model::{BaseTokenConversionRatio, ConversionRatio},
};

/// Represents a row in the `base_token_ratios` table.
#[derive(Debug, Clone)]
pub struct StorageBaseTokenRatio {
    pub id: i64,
    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
    pub ratio_timestamp: NaiveDateTime,
    pub numerator_l1: BigDecimal,
    pub denominator_l1: BigDecimal,
    pub numerator_sl: BigDecimal,
    pub denominator_sl: BigDecimal,
    pub used_in_l1: bool,
}

impl From<StorageBaseTokenRatio> for BaseTokenRatio {
    fn from(row: StorageBaseTokenRatio) -> BaseTokenRatio {
        BaseTokenRatio {
            id: row.id as u32,
            ratio_timestamp: row.ratio_timestamp.and_utc(),
            ratio: BaseTokenConversionRatio {
                l1: ConversionRatio {
                    numerator: NonZeroU64::new(
                        row.numerator_l1.to_u64().expect("numerator is not u64"),
                    )
                    .unwrap(),
                    denominator: NonZeroU64::new(
                        row.denominator_l1.to_u64().expect("denominator is not u64"),
                    )
                    .unwrap(),
                },
                sl: ConversionRatio {
                    numerator: NonZeroU64::new(
                        row.numerator_sl.to_u64().expect("numerator is not u64"),
                    )
                    .unwrap(),
                    denominator: NonZeroU64::new(
                        row.denominator_sl.to_u64().expect("denominator is not u64"),
                    )
                    .unwrap(),
                },
            },
            used_in_l1: row.used_in_l1,
        }
    }
}
