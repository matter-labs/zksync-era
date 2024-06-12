use bigdecimal::BigDecimal;
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};

use crate::{models::storage_base_token_price::StorageBaseTokenPrice, Core};

#[derive(Debug)]
pub struct BaseTokenDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl BaseTokenDal<'_, '_> {
    pub async fn insert_token_price(
        &mut self,
        numerator: &BigDecimal,
        denominator: &BigDecimal,
        ratio_timestamp: &chrono::NaiveDateTime,
    ) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                base_token_prices (numerator, denominator, ratio_timestamp)
            VALUES
                ($1, $2, $3)
            "#,
            numerator,
            denominator,
            ratio_timestamp,
        )
        .instrument("insert_base_token_price")
        .execute(self.storage)
        .await?;
        Ok(())
    }

    // TODO (PE-128): pub async fn mark_l1_update()

    pub async fn get_latest_price(&mut self) -> DalResult<StorageBaseTokenPrice> {
        let row = sqlx::query_as!(
            StorageBaseTokenPrice,
            r#"
            SELECT
                *
            FROM
                base_token_prices
            ORDER BY
                created_at DESC
            LIMIT
                1
            "#,
        )
        .instrument("get_latest_base_token_price")
        .fetch_one(self.storage)
        .await?;
        Ok(row)
    }
}
