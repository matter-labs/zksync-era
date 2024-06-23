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
        base_token_price: &BigDecimal,
        eth_price: &BigDecimal,
        ratio_timestamp: &chrono::NaiveDateTime,
    ) -> DalResult<usize> {
        let row = sqlx::query!(
            r#"
            INSERT INTO
                base_token_prices (base_token_price, eth_price, ratio_timestamp, created_at, updated_at)
            VALUES
                ($1, $2, $3, NOW(), NOW())
            RETURNING
                id
            "#,
            base_token_price,
            eth_price,
            ratio_timestamp,
        )
        .instrument("insert_base_token_price")
        .fetch_one(self.storage)
        .await?;

        Ok(row.id as usize)
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
