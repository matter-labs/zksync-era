use sqlx::types::chrono::Utc;
use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{CopyStatement, InstrumentExt},
    write_str, writeln_str,
};
use zksync_types::{tokens::TokenInfo, Address, L2BlockNumber};

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
    pub async fn roll_back_tokens(&mut self, block_number: L2BlockNumber) -> DalResult<()> {
        let all_token_addresses = self.get_all_l2_token_addresses().await?;
        let token_deployment_data = self
            .storage
            .storage_logs_dal()
            .filter_deployed_contracts(all_token_addresses.iter().copied(), None)
            .await?;
        let token_addresses_to_be_removed: Vec<_> = all_token_addresses
            .into_iter()
            .filter_map(|address| {
                if address.is_zero() {
                    None
                } else if let Some(deployed_at) = token_deployment_data.get(&address) {
                    (deployed_at > &block_number).then_some(address.0)
                } else {
                    // Token belongs to a "pending" L2 block that's not yet fully inserted to the database.
                    Some(address.0)
                }
            })
            .collect();
        sqlx::query!(
            r#"
            DELETE FROM tokens
            WHERE
                l2_address = ANY ($1)
            "#,
            &token_addresses_to_be_removed as &[_]
        )
        .instrument("roll_back_tokens")
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
    use zksync_types::{get_code_key, tokens::TokenMetadata, ProtocolVersion, StorageLog, H256};

    use super::*;
    use crate::{tests::create_l2_block_header, ConnectionPool, Core, CoreDal};

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

    async fn insert_l2_block(conn: &mut Connection<'_, Core>, number: u32, logs: Vec<StorageLog>) {
        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(number))
            .await
            .unwrap();

        conn.storage_logs_dal()
            .insert_storage_logs(L2BlockNumber(number), &logs)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn adding_and_getting_tokens() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut storage = pool.connection().await.unwrap();
        storage
            .protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let tokens = [test_token_info(), eth_token_info()];
        storage.tokens_dal().add_tokens(&tokens).await.unwrap();

        let storage_logs: Vec<_> = tokens
            .iter()
            .map(|token_info| {
                StorageLog::new_write_log(
                    get_code_key(&token_info.l2_address),
                    H256::repeat_byte(0xff),
                )
            })
            .collect();
        insert_l2_block(&mut storage, 1, storage_logs).await;

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
        storage
            .protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let eth_info = eth_token_info();
        let eth_deployment_log =
            StorageLog::new_write_log(get_code_key(&eth_info.l2_address), H256::repeat_byte(1));
        storage
            .tokens_dal()
            .add_tokens(slice::from_ref(&eth_info))
            .await
            .unwrap();
        insert_l2_block(&mut storage, 0, vec![eth_deployment_log]).await;

        let test_info = test_token_info();
        let test_deployment_log =
            StorageLog::new_write_log(get_code_key(&test_info.l2_address), H256::repeat_byte(2));
        storage
            .tokens_dal()
            .add_tokens(slice::from_ref(&test_info))
            .await
            .unwrap();
        insert_l2_block(&mut storage, 2, vec![test_deployment_log]).await;

        test_getting_all_tokens(&mut storage).await;

        storage
            .tokens_dal()
            .roll_back_tokens(L2BlockNumber(2))
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
            .roll_back_tokens(L2BlockNumber(1))
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
        for at_l2_block in [None, Some(L2BlockNumber(2)), Some(L2BlockNumber(100))] {
            let all_tokens = storage
                .tokens_web3_dal()
                .get_all_tokens(at_l2_block)
                .await
                .unwrap();
            assert_eq!(all_tokens.len(), 2);
            assert!(all_tokens.contains(&eth_token_info()));
            assert!(all_tokens.contains(&test_token_info()));
        }

        for at_l2_block in [L2BlockNumber(0), L2BlockNumber(1)] {
            let all_tokens = storage
                .tokens_web3_dal()
                .get_all_tokens(Some(at_l2_block))
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
            .insert_storage_logs(L2BlockNumber(1), &[failed_deployment_log])
            .await
            .unwrap();

        let test_deployment_log =
            StorageLog::new_write_log(get_code_key(&test_info.l2_address), H256::repeat_byte(2));
        storage
            .storage_logs_dal()
            .insert_storage_logs(L2BlockNumber(100), &[test_deployment_log])
            .await
            .unwrap();
        storage
            .tokens_dal()
            .add_tokens(slice::from_ref(&test_info))
            .await
            .unwrap();

        // Sanity check: before revert the token must be present.
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
            .roll_back_tokens(L2BlockNumber(99))
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
