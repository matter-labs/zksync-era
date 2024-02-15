use sqlx::types::chrono::Utc;
use zksync_types::{
    tokens::TokenInfo, Address, MiniblockNumber, ACCOUNT_CODE_STORAGE_ADDRESS,
    FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH,
};

use crate::StorageProcessor;

#[derive(Debug)]
pub struct TokensDal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl TokensDal<'_, '_> {
    pub async fn add_tokens(&mut self, tokens: &[TokenInfo]) -> sqlx::Result<()> {
        let mut copy = self
            .storage
            .conn()
            .copy_in_raw(
                "COPY tokens (l1_address, l2_address, name, symbol, decimals, well_known, created_at, updated_at)
                FROM STDIN WITH (DELIMITER '|')",
            )
            .await?;

        let mut buffer = String::new();
        let now = Utc::now().naive_utc().to_string();
        for token_info in tokens {
            write_str!(
                &mut buffer,
                "\\\\x{:x}|\\\\x{:x}|",
                token_info.l1_address,
                token_info.l2_address
            );
            writeln_str!(
                &mut buffer,
                "{}|{}|{}|FALSE|{now}|{now}",
                token_info.metadata.name,
                token_info.metadata.symbol,
                token_info.metadata.decimals
            );
        }
        copy.send(buffer.as_bytes()).await?;
        copy.finish().await?;
        Ok(())
    }

    pub async fn mark_token_as_well_known(&mut self, l1_address: Address) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            UPDATE tokens
            SET
                well_known = TRUE,
                updated_at = NOW()
            WHERE
                l1_address = $1
            "#,
            l1_address.as_bytes()
        )
        .execute(self.storage.conn())
        .await?;
        Ok(())
    }

    pub async fn get_all_l2_token_addresses(&mut self) -> sqlx::Result<Vec<Address>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                l2_address
            FROM
                tokens
            "#
        )
        .fetch_all(self.storage.conn())
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Address::from_slice(&row.l2_address))
            .collect())
    }

    pub async fn rollback_tokens(&mut self, block_number: MiniblockNumber) -> sqlx::Result<()> {
        sqlx::query!(
            r#"
            DELETE FROM tokens
            WHERE
                l2_address IN (
                    SELECT
                        SUBSTRING(key, 12, 20)
                    FROM
                        storage_logs
                    WHERE
                        storage_logs.address = $1
                        AND miniblock_number > $2
                        AND NOT EXISTS (
                            SELECT
                                1
                            FROM
                                storage_logs AS s
                            WHERE
                                s.hashed_key = storage_logs.hashed_key
                                AND (s.miniblock_number, s.operation_number) >= (storage_logs.miniblock_number, storage_logs.operation_number)
                                AND s.value = $3
                        )
                )
            "#,
            ACCOUNT_CODE_STORAGE_ADDRESS.as_bytes(),
            block_number.0 as i64,
            FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes()
        )
        .execute(self.storage.conn())
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use zksync_types::tokens::TokenMetadata;

    use super::*;
    use crate::ConnectionPool;

    #[tokio::test]
    async fn adding_and_getting_tokens() {
        let pool = ConnectionPool::test_pool().await;
        let mut storage = pool.access_storage().await.unwrap();
        let tokens = [
            TokenInfo {
                l1_address: Address::repeat_byte(1),
                l2_address: Address::repeat_byte(2),
                metadata: TokenMetadata {
                    name: "Test".to_string(),
                    symbol: "TST".to_string(),
                    decimals: 10,
                },
            },
            TokenInfo {
                l1_address: Address::repeat_byte(0),
                l2_address: Address::repeat_byte(0),
                metadata: TokenMetadata {
                    name: "Ether".to_string(),
                    symbol: "ETH".to_string(),
                    decimals: 18,
                },
            },
        ];
        storage.tokens_dal().add_tokens(&tokens).await.unwrap();

        let token_addresses = storage
            .tokens_dal()
            .get_all_l2_token_addresses()
            .await
            .unwrap();
        assert_eq!(
            token_addresses.into_iter().collect::<HashSet<_>>(),
            tokens
                .iter()
                .map(|token| token.l2_address)
                .collect::<HashSet<_>>(),
        );

        for token in &tokens {
            storage
                .tokens_dal()
                .mark_token_as_well_known(token.l1_address)
                .await
                .unwrap();
        }

        let well_known_tokens = storage
            .tokens_web3_dal()
            .get_well_known_tokens()
            .await
            .unwrap();
        assert_eq!(well_known_tokens.len(), 2);
        assert!(well_known_tokens.contains(&tokens[0]));
        assert!(well_known_tokens.contains(&tokens[1]));
    }
}
