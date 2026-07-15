use bigdecimal::{BigDecimal, FromPrimitive};
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{base_token_ratio::BaseTokenRatio, fee_model::BaseTokenConversionRatio};

use crate::{models::storage_base_token_ratio::StorageBaseTokenRatio, Core};

#[derive(Debug)]
pub struct BaseTokenDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl BaseTokenDal<'_, '_> {
    pub async fn insert_token_ratio(
        &mut self,
        base_token_conversion_ratio: BaseTokenConversionRatio,
        ratio_timestamp: &chrono::NaiveDateTime,
    ) -> DalResult<usize> {
        let row = sqlx::query!(
            r#"
            INSERT INTO
            base_token_ratios (
                numerator_l1,
                denominator_l1,
                numerator_sl,
                denominator_sl,
                ratio_timestamp,
                created_at,
                updated_at
            )
            VALUES
            ($1, $2, $3, $4, $5, NOW(), NOW())
            RETURNING
            id
            "#,
            BigDecimal::from_u64(
                base_token_conversion_ratio
                    .l1_conversion_ratio()
                    .numerator
                    .get()
            ),
            BigDecimal::from_u64(
                base_token_conversion_ratio
                    .l1_conversion_ratio()
                    .denominator
                    .get()
            ),
            BigDecimal::from_u64(
                base_token_conversion_ratio
                    .sl_conversion_ratio()
                    .numerator
                    .get()
            ),
            BigDecimal::from_u64(
                base_token_conversion_ratio
                    .sl_conversion_ratio()
                    .denominator
                    .get()
            ),
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
