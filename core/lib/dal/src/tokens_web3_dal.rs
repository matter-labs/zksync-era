use crate::models::storage_token::StorageTokenPrice;
use crate::SqlxError;
use crate::StorageProcessor;
use zksync_types::{
    tokens::{TokenInfo, TokenMetadata, TokenPrice},
    Address,
};

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
}
