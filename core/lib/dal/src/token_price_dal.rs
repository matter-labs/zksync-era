use std::str::FromStr;

use chrono::{DateTime, NaiveDateTime};
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
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
    pub async fn fetch_ratio(&mut self, token: Address) -> anyhow::Result<Option<TokenPriceData>> {
        todo!("TokenPriceDal::fetch_ratio");

        let temp = TokenPriceData {
            token: Address::from_str("").expect("Invalid address"),
            rate: BigDecimal::from(0),
            timestamp: 0,
        };
        Ok(Some(temp))
    }

    pub async fn insert_ratio(&mut self, token_price_data: TokenPriceData) -> anyhow::Result<()> {
        sqlx::query!(
            r#"
            INSERT INTO
                token_price_ratio (token_address, ratio, updated_at)
            VALUES
                ($1, $2, $3)
            "#,
            token_price_data.token.as_bytes(),
            token_price_data.rate.to_string(),
            NaiveDateTime::default(), // TODO: replace with actual timestamp
        )
        .instrument("insert_token_price_data")
        .execute(self.storage)
        .await?;
        Ok(())
    }
}
