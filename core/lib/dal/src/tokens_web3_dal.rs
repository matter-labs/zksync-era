use crate::models::storage_token::{StorageTokenMetadata, StorageTokenPrice};
use crate::SqlxError;
use crate::StorageProcessor;
use num::{rational::Ratio, BigUint};
use sqlx::postgres::types::PgInterval;
use zksync_types::{
    tokens::{TokenInfo, TokenMetadata, TokenPrice},
    Address,
};
use zksync_utils::ratio_to_big_decimal;

// Precision of the USD price per token
pub(crate) const STORED_USD_PRICE_PRECISION: usize = 6;

#[derive(Debug)]
pub struct TokensWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl TokensWeb3Dal<'_, '_> {
    pub async fn get_well_known_tokens(&mut self) -> Result<Vec<TokenInfo>, SqlxError> {
        {
            let records = sqlx::query!(
                "SELECT l1_address, l2_address, name, symbol, decimals FROM tokens
                 WHERE well_known = true
                 ORDER BY symbol"
            )
            .fetch_all(self.storage.conn())
            .await?;
            let result: Vec<TokenInfo> = records
                .into_iter()
                .map(|record| TokenInfo {
                    l1_address: Address::from_slice(&record.l1_address),
                    l2_address: Address::from_slice(&record.l2_address),
                    metadata: TokenMetadata {
                        name: record.name,
                        symbol: record.symbol,
                        decimals: record.decimals as u8,
                    },
                })
                .collect();
            Ok(result)
        }
    }

    pub async fn is_token_actively_trading(
        &mut self,
        l2_token: &Address,
        min_volume: &Ratio<BigUint>,
        max_acceptable_volume_age_in_secs: u32,
        max_acceptable_price_age_in_secs: u32,
    ) -> Result<bool, SqlxError> {
        {
            let min_volume = ratio_to_big_decimal(min_volume, STORED_USD_PRICE_PRECISION);
            let volume_pg_interval = PgInterval {
                months: 0,
                days: 0,
                microseconds: (max_acceptable_volume_age_in_secs as i64) * 1000000,
            };
            let price_pg_interval = PgInterval {
                months: 0,
                days: 0,
                microseconds: (max_acceptable_price_age_in_secs as i64) * 1000000,
            };
            let count = sqlx::query!(
                r#"
                SELECT COUNT(*) as "count!" FROM tokens
                WHERE l2_address = $1 AND
                    market_volume > $2 AND now() - market_volume_updated_at < $3 AND
                    usd_price > 0 AND now() - usd_price_updated_at < $4
                "#,
                l2_token.as_bytes(),
                min_volume,
                volume_pg_interval,
                price_pg_interval
            )
            .fetch_one(self.storage.conn())
            .await
            .unwrap()
            .count;
            Ok(count == 1)
        }
    }

    pub async fn get_token_price(
        &mut self,
        l2_address: &Address,
    ) -> Result<Option<TokenPrice>, SqlxError> {
        {
            let storage_price = sqlx::query_as!(
                StorageTokenPrice,
                "SELECT usd_price, usd_price_updated_at FROM tokens WHERE l2_address = $1",
                l2_address.as_bytes(),
            )
            .fetch_optional(self.storage.conn())
            .await?;

            Ok(storage_price.and_then(Into::into))
        }
    }

    pub async fn get_token_metadata(
        &mut self,
        l2_address: &Address,
    ) -> Result<Option<TokenMetadata>, SqlxError> {
        {
            let storage_token_metadata = sqlx::query_as!(
                StorageTokenMetadata,
                r#"
                SELECT
                    COALESCE(token_list_name, name) as "name!",
                    COALESCE(token_list_symbol, symbol) as "symbol!",
                    COALESCE(token_list_decimals, decimals) as "decimals!"
                FROM tokens WHERE l2_address = $1
                "#,
                l2_address.as_bytes(),
            )
            .fetch_optional(self.storage.conn())
            .await?;

            Ok(storage_token_metadata.map(Into::into))
        }
    }
}
