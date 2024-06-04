use std::str::FromStr;

use chrono::{DateTime, NaiveDateTime};
use zksync_db_connection::{
    connection::Connection,
    instrument::{CopyStatement, InstrumentExt},
    write_str, writeln_str,
};
use zksync_types::{
    tokens::{TokenInfo, TokenPriceData},
    Address, L2BlockNumber,
};

use crate::{BigDecimal, Core, CoreDal};

#[derive(Debug)]
pub struct TokenPriceDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl TokenPriceDal<'_, '_> {
    pub async fn fetch_ratio(
        &mut self,
        token_address: Address,
    ) -> anyhow::Result<Option<BigDecimal>> {
        let ratio = sqlx::query!(
            r#"
            SELECT
                ratio
            FROM
                token_price_ratio
            WHERE
                token_address = $1
            "#,
            token_address.as_bytes(),
        )
        .instrument("fetch_token_price_data")
        .fetch_optional(self.storage)
        .await?;
        Ok(ratio.map(|r| BigDecimal::from_str(&r.ratio).unwrap()))
    }

    pub async fn insert_ratio(&mut self, token_price_data: TokenPriceData) -> anyhow::Result<()> {
        // Attempt to update token price data
        let updated = sqlx::query!(
            r#"
            UPDATE token_price_ratio
            SET
                ratio = $2,
                updated_at = NOW()
            WHERE
                token_address = $1
            RETURNING
                *
            "#,
            token_price_data.address.as_bytes(),
            token_price_data.rate.to_string(),
        )
        .instrument("update_token_price_data")
        .fetch_optional(self.storage)
        .await?;
        if updated.is_some() {
            return Ok(());
        }

        // Token has no previous price data, insert new row
        sqlx::query!(
            r#"
            INSERT INTO
                token_price_ratio (token_address, ratio, updated_at)
            VALUES
                ($1, $2, NOW())
            "#,
            token_price_data.address.as_bytes(),
            token_price_data.rate.to_string(),
        )
        .instrument("insert_token_price_data")
        .execute(self.storage)
        .await?;
        Ok(())
    }
}
