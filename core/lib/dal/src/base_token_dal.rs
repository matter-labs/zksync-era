use std::num::NonZeroU64;

use bigdecimal::{BigDecimal, FromPrimitive};
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::base_token_ratio::BaseTokenRatio;

use crate::{models::storage_base_token_ratio::StorageBaseTokenRatio, Core};

#[derive(Debug)]
pub struct BaseTokenDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl BaseTokenDal<'_, '_> {
    pub async fn insert_token_ratio(
        &mut self,
        numerator: NonZeroU64,
        denominator: NonZeroU64,
        ratio_timestamp: &chrono::NaiveDateTime,
    ) -> DalResult<usize> {
        let row = sqlx::query!(
            r#"
            INSERT INTO
                base_token_ratios (numerator, denominator, ratio_timestamp, created_at, updated_at)
            VALUES
                ($1, $2, $3, NOW(), NOW())
            RETURNING
                id
            "#,
            BigDecimal::from_u64(numerator.get()),
            BigDecimal::from_u64(denominator.get()),
            ratio_timestamp,
        )
        .instrument("insert_token_ratio")
        .fetch_one(self.storage)
        .await?;

        Ok(row.id as usize)
    }

    pub async fn get_latest_ratio(&mut self) -> DalResult<Option<BaseTokenRatio>> {
        let row = sqlx::query_as!(
            StorageBaseTokenRatio,
            r#"
            SELECT
                *
            FROM
                base_token_ratios
            ORDER BY
                ratio_timestamp DESC
            LIMIT
                1
            "#,
        )
        .instrument("get_latest_ratio")
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(|r| r.into()))
    }
}
