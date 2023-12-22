use zksync_types::{tokens::TokenPrice, Address};

use crate::{models::storage_token::StorageTokenPrice, SqlxError, StorageProcessor};

#[derive(Debug)]
pub struct TokensWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl TokensWeb3Dal<'_, '_> {
    pub async fn get_token_price(
        &mut self,
        l2_address: &Address,
    ) -> Result<Option<TokenPrice>, SqlxError> {
        {
            let storage_price = sqlx::query_as!(
                StorageTokenPrice,
                r#"
                SELECT
                    usd_price,
                    usd_price_updated_at
                FROM
                    tokens
                WHERE
                    l2_address = $1
                "#,
                l2_address.as_bytes(),
            )
            .fetch_optional(self.storage.conn())
            .await?;

            Ok(storage_price.and_then(Into::into))
        }
    }
}
