use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_types::{
    tokens::{TokenInfo, TokenMetadata},
    Address, L2BlockNumber,
};

use crate::{Core, CoreDal};

#[derive(Debug)]
struct StorageTokenInfo {
    l1_address: Vec<u8>,
    l2_address: Vec<u8>,
    name: String,
    symbol: String,
    decimals: i32,
}

impl From<StorageTokenInfo> for TokenInfo {
    fn from(row: StorageTokenInfo) -> Self {
        Self {
            l1_address: Address::from_slice(&row.l1_address),
            l2_address: Address::from_slice(&row.l2_address),
            metadata: TokenMetadata {
                name: row.name,
                symbol: row.symbol,
                decimals: row.decimals as u8,
            },
        }
    }
}

#[derive(Debug)]
pub struct TokensWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl TokensWeb3Dal<'_, '_> {
    /// Returns information about all tokens.
    pub async fn get_all_tokens(
        &mut self,
        at_l2_block: Option<L2BlockNumber>,
    ) -> DalResult<Vec<TokenInfo>> {
        let records = sqlx::query_as!(
            StorageTokenInfo,
            r#"
            SELECT
                l1_address,
                l2_address,
                name,
                symbol,
                decimals
            FROM
                tokens
            ORDER BY
                symbol
            "#
        )
        .instrument("get_all_tokens")
        .with_arg("at_l2_block", &at_l2_block)
        .report_latency()
        .fetch_all(self.storage)
        .await?;

        let mut all_tokens: Vec<_> = records.into_iter().map(TokenInfo::from).collect();

        let token_addresses = all_tokens.iter().map(|token| token.l2_address);
        let filtered_addresses = self
            .storage
            .storage_logs_dal()
            .filter_deployed_contracts(token_addresses, at_l2_block)
            .await?;

        all_tokens.retain(|token| {
            token.l2_address.is_zero() || filtered_addresses.contains_key(&token.l2_address)
        });
        Ok(all_tokens)
    }
}
