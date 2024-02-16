use zksync_types::{
    tokens::{TokenInfo, TokenMetadata},
    Address,
};

use crate::StorageProcessor;

#[derive(Debug)]
pub struct TokensWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl TokensWeb3Dal<'_, '_> {
    pub async fn get_well_known_tokens(&mut self) -> sqlx::Result<Vec<TokenInfo>> {
        let records = sqlx::query!(
            r#"
            SELECT
                l1_address,
                l2_address,
                NAME,
                symbol,
                decimals
            FROM
                tokens
            WHERE
                well_known = TRUE
            ORDER BY
                symbol
            "#
        )
        .fetch_all(self.storage.conn())
        .await?;

        Ok(records
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
            .collect())
    }
}
