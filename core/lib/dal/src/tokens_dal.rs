use sqlx::types::chrono::Utc;
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{CopyStatement, InstrumentExt},
    write_str, writeln_str,
};
use zksync_types::{tokens::TokenInfo, Address, MiniblockNumber};

use crate::{Core, CoreDal};

#[derive(Debug)]
pub struct TokensDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl TokensDal<'_, '_> {
    pub async fn add_tokens(&mut self, tokens: &[TokenInfo]) -> DalResult<()> {
        let tokens_len = tokens.len();
        let copy = CopyStatement::new(
            "COPY tokens (l1_address, l2_address, name, symbol, decimals, well_known, created_at, updated_at)
             FROM STDIN WITH (DELIMITER '|')",
        )
        .instrument("add_tokens")
        .with_arg("tokens.len", &tokens_len)
        .start(self.storage)
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
        copy.send(buffer.as_bytes()).await
    }

    pub async fn mark_token_as_well_known(&mut self, l1_address: Address) -> DalResult<()> {
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
        .instrument("mark_token_as_well_known")
        .with_arg("l1_address", &l1_address)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn get_all_l2_token_addresses(&mut self) -> DalResult<Vec<Address>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                l2_address
            FROM
                tokens
            "#
        )
        .instrument("get_all_l2_token_addresses")
        .report_latency()
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| Address::from_slice(&row.l2_address))
            .collect())
    }

    /// Removes token records that were deployed after `block_number`.
    pub async fn rollback_tokens(&mut self, block_number: MiniblockNumber) -> DalResult<()> {
        let all_token_addresses = self.get_all_l2_token_addresses().await?;
        let token_deployment_data = self
            .storage
            .storage_logs_dal()
            .filter_deployed_contracts(all_token_addresses.into_iter(), None)
            .await?;
        let token_addresses_to_be_removed: Vec<_> = token_deployment_data
            .into_iter()
            .filter_map(|(address, deployed_at)| (deployed_at > block_number).then_some(address.0))
            .collect();
        sqlx::query!(
            r#"
            DELETE FROM tokens
            WHERE
                l2_address = ANY ($1)
            "#,
            &token_addresses_to_be_removed as &[_]
        )
        .instrument("rollback_tokens")
        .with_arg("block_number", &block_number)
        .with_arg(
            "token_addresses_to_be_removed.len",
            &token_addresses_to_be_removed.len(),
        )
        .execute(self.storage)
        .await?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, slice};

    use zksync_system_constants::FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH;
    use zksync_types::{get_code_key, tokens::TokenMetadata, StorageLog, H256};

    use super::*;
    use crate::{ConnectionPool, Core, CoreDal};

    fn test_token_info() -> TokenInfo {
        TokenInfo {
            l1_address: Address::repeat_byte(1),
            l2_address: Address::repeat_byte(2),
            metadata: TokenMetadata {
                name: "Test".to_string(),
                symbol: "TST".to_string(),
                decimals: 10,
            },
        }
    }

    fn eth_token_info() -> TokenInfo {
        TokenInfo {
            l1_address: Address::repeat_byte(0),
            l2_address: Address::repeat_byte(0),
            metadata: TokenMetadata {
                name: "Ether".to_string(),
                symbol: "ETH".to_string(),
                decimals: 18,
            },
        }
    }

    #[tokio::test]
    async fn adding_and_getting_tokens() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        let tokens = [test_token_info(), eth_token_info()];
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

        let all_tokens = storage
            .tokens_web3_dal()
            .get_all_tokens(None)
            .await
            .unwrap();
        assert_eq!(all_tokens.len(), 2);
        assert!(all_tokens.contains(&tokens[0]));
        assert!(all_tokens.contains(&tokens[1]));

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

    #[tokio::test]
    async fn rolling_back_tokens() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();

        let eth_info = eth_token_info();
        let eth_deployment_log =
            StorageLog::new_write_log(get_code_key(&eth_info.l2_address), H256::repeat_byte(1));
        storage
            .storage_logs_dal()
            .insert_storage_logs(
                MiniblockNumber(0),
                &[(H256::zero(), vec![eth_deployment_log])],
            )
            .await
            .unwrap();
        storage
            .tokens_dal()
            .add_tokens(slice::from_ref(&eth_info))
            .await
            .unwrap();

        let test_info = test_token_info();
        let test_deployment_log =
            StorageLog::new_write_log(get_code_key(&test_info.l2_address), H256::repeat_byte(2));
        storage
            .storage_logs_dal()
            .insert_storage_logs(
                MiniblockNumber(2),
                &[(H256::zero(), vec![test_deployment_log])],
            )
            .await
            .unwrap();
        storage
            .tokens_dal()
            .add_tokens(slice::from_ref(&test_info))
            .await
            .unwrap();

        test_getting_all_tokens(&mut storage).await;

        storage
            .tokens_dal()
            .rollback_tokens(MiniblockNumber(2))
            .await
            .unwrap();
        // Should be a no-op.
        assert_eq!(
            storage
                .tokens_dal()
                .get_all_l2_token_addresses()
                .await
                .unwrap(),
            [eth_info.l2_address, test_info.l2_address]
        );

        storage
            .tokens_dal()
            .rollback_tokens(MiniblockNumber(1))
            .await
            .unwrap();
        // The custom token should be removed; Ether shouldn't.
        assert_eq!(
            storage
                .tokens_dal()
                .get_all_l2_token_addresses()
                .await
                .unwrap(),
            [eth_info.l2_address]
        );
    }

    async fn test_getting_all_tokens(storage: &mut Connection<'_, Core>) {
        for at_miniblock in [None, Some(MiniblockNumber(2)), Some(MiniblockNumber(100))] {
            let all_tokens = storage
                .tokens_web3_dal()
                .get_all_tokens(at_miniblock)
                .await
                .unwrap();
            assert_eq!(all_tokens.len(), 2);
            assert!(all_tokens.contains(&eth_token_info()));
            assert!(all_tokens.contains(&test_token_info()));
        }

        for at_miniblock in [MiniblockNumber(0), MiniblockNumber(1)] {
            let all_tokens = storage
                .tokens_web3_dal()
                .get_all_tokens(Some(at_miniblock))
                .await
                .unwrap();
            assert_eq!(all_tokens, [eth_token_info()]);
        }
    }

    #[tokio::test]
    async fn rolling_back_tokens_with_failed_deployment() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();

        let test_info = test_token_info();

        // Emulate failed deployment.
        let failed_deployment_log = StorageLog::new_write_log(
            get_code_key(&test_info.l2_address),
            FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH,
        );
        storage
            .storage_logs_dal()
            .insert_storage_logs(
                MiniblockNumber(1),
                &[(H256::zero(), vec![failed_deployment_log])],
            )
            .await
            .unwrap();

        let test_deployment_log =
            StorageLog::new_write_log(get_code_key(&test_info.l2_address), H256::repeat_byte(2));
        storage
            .storage_logs_dal()
            .insert_storage_logs(
                MiniblockNumber(100),
                &[(H256::zero(), vec![test_deployment_log])],
            )
            .await
            .unwrap();
        storage
            .tokens_dal()
            .add_tokens(slice::from_ref(&test_info))
            .await
            .unwrap();

        // Sanity check: before rollback the token must be present.
        assert_eq!(
            storage
                .tokens_dal()
                .get_all_l2_token_addresses()
                .await
                .unwrap(),
            [test_info.l2_address]
        );

        storage
            .tokens_dal()
            .rollback_tokens(MiniblockNumber(99))
            .await
            .unwrap();
        // Token must be removed despite it's failed deployment being earlier than the last retained miniblock.
        assert_eq!(
            storage
                .tokens_dal()
                .get_all_l2_token_addresses()
                .await
                .unwrap(),
            []
        );
    }
}
