use sqlx::types::chrono::NaiveDateTime;

use zksync_types::{
    api, Address, L2ChainId, MiniblockNumber, Transaction, ACCOUNT_CODE_STORAGE_ADDRESS,
    FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH, H160, H256, U256, U64,
};
use zksync_utils::{bigdecimal_to_u256, h256_to_account_address};

use crate::models::{
    storage_block::{bind_block_where_sql_params, web3_block_where_sql},
    storage_event::StorageWeb3Log,
    storage_transaction::{
        extract_web3_transaction, web3_transaction_select_sql, StorageTransaction,
        StorageTransactionDetails,
    },
};
use crate::{instrument::InstrumentExt, SqlxError, StorageProcessor};

#[derive(Debug)]
pub struct TransactionsWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl TransactionsWeb3Dal<'_, '_> {
    pub async fn get_transaction_receipt(
        &mut self,
        hash: H256,
    ) -> Result<Option<api::TransactionReceipt>, SqlxError> {
        {
            let receipt = sqlx::query!(
                r#"
                WITH sl AS (
                    SELECT * FROM storage_logs
                    WHERE storage_logs.address = $1 AND storage_logs.tx_hash = $2
                    ORDER BY storage_logs.miniblock_number DESC, storage_logs.operation_number DESC
                    LIMIT 1
                )
                SELECT
                     transactions.hash as tx_hash,
                     transactions.index_in_block as index_in_block,
                     transactions.l1_batch_tx_index as l1_batch_tx_index,
                     transactions.miniblock_number as block_number,
                     transactions.error as error,
                     transactions.effective_gas_price as effective_gas_price,
                     transactions.initiator_address as initiator_address,
                     transactions.data->'to' as "transfer_to?",
                     transactions.data->'contractAddress' as "execute_contract_address?",
                     transactions.tx_format as "tx_format?",
                     transactions.refunded_gas as refunded_gas,
                     transactions.gas_limit as gas_limit,
                     miniblocks.hash as "block_hash?",
                     miniblocks.l1_batch_number as "l1_batch_number?",
                     sl.key as "contract_address?"
                FROM transactions
                LEFT JOIN miniblocks
                    ON miniblocks.number = transactions.miniblock_number
                LEFT JOIN sl
                    ON sl.value != $3
                WHERE transactions.hash = $2
                "#,
                ACCOUNT_CODE_STORAGE_ADDRESS.as_bytes(),
                hash.as_bytes(),
                FAILED_CONTRACT_DEPLOYMENT_BYTECODE_HASH.as_bytes()
            )
            .instrument("get_transaction_receipt")
            .with_arg("hash", &hash)
            .fetch_optional(self.storage.conn())
            .await?
            .map(|db_row| {
                let status = match (db_row.block_number, db_row.error) {
                    (_, Some(_)) => Some(U64::from(0)),
                    (Some(_), None) => Some(U64::from(1)),
                    // tx not executed yet
                    _ => None,
                };
                let tx_type = db_row.tx_format.map(U64::from).unwrap_or_default();
                let transaction_index = db_row.index_in_block.map(U64::from).unwrap_or_default();

                let block_hash = db_row.block_hash.map(|bytes| H256::from_slice(&bytes));
                api::TransactionReceipt {
                    transaction_hash: H256::from_slice(&db_row.tx_hash),
                    transaction_index,
                    block_hash,
                    block_number: db_row.block_number.map(U64::from),
                    l1_batch_tx_index: db_row.l1_batch_tx_index.map(U64::from),
                    l1_batch_number: db_row.l1_batch_number.map(U64::from),
                    from: H160::from_slice(&db_row.initiator_address),
                    to: db_row
                        .transfer_to
                        .or(db_row.execute_contract_address)
                        .map(|addr| {
                            serde_json::from_value::<Address>(addr)
                                .expect("invalid address value in the database")
                        })
                        // For better compatibility with various clients, we never return null.
                        .or_else(|| Some(Address::default())),
                    cumulative_gas_used: Default::default(), // TODO: Should be actually calculated (SMA-1183).
                    gas_used: {
                        let refunded_gas: U256 = db_row.refunded_gas.into();
                        db_row.gas_limit.map(|val| {
                            let gas_limit = bigdecimal_to_u256(val);
                            gas_limit - refunded_gas
                        })
                    },
                    effective_gas_price: Some(
                        db_row
                            .effective_gas_price
                            .map(bigdecimal_to_u256)
                            .unwrap_or_default(),
                    ),
                    contract_address: db_row
                        .contract_address
                        .map(|addr| h256_to_account_address(&H256::from_slice(&addr))),
                    logs: vec![],
                    l2_to_l1_logs: vec![],
                    status,
                    root: block_hash,
                    logs_bloom: Default::default(),
                    // Even though the Rust SDK recommends us to supply "None" for legacy transactions
                    // we always supply some number anyway to have the same behaviour as most popular RPCs
                    transaction_type: Some(tx_type),
                }
            });
            match receipt {
                Some(mut receipt) => {
                    let logs: Vec<_> = sqlx::query_as!(
                        StorageWeb3Log,
                        r#"
                        SELECT
                            address, topic1, topic2, topic3, topic4, value,
                            Null::bytea as "block_hash", Null::bigint as "l1_batch_number?",
                            miniblock_number, tx_hash, tx_index_in_block,
                            event_index_in_block, event_index_in_tx
                        FROM events
                        WHERE tx_hash = $1
                        ORDER BY miniblock_number ASC, event_index_in_block ASC
                        "#,
                        hash.as_bytes()
                    )
                    .instrument("get_transaction_receipt_events")
                    .with_arg("hash", &hash)
                    .fetch_all(self.storage.conn())
                    .await?
                    .into_iter()
                    .map(|storage_log| {
                        let mut log = api::Log::from(storage_log);
                        log.block_hash = receipt.block_hash;
                        log.l1_batch_number = receipt.l1_batch_number;
                        log
                    })
                    .collect();

                    receipt.logs = logs;

                    let l2_to_l1_logs = self.storage.events_dal().l2_to_l1_logs(hash).await?;
                    let l2_to_l1_logs: Vec<_> = l2_to_l1_logs
                        .into_iter()
                        .map(|storage_l2_to_l1_log| {
                            let mut l2_to_l1_log = api::L2ToL1Log::from(storage_l2_to_l1_log);
                            l2_to_l1_log.block_hash = receipt.block_hash;
                            l2_to_l1_log.l1_batch_number = receipt.l1_batch_number;
                            l2_to_l1_log
                        })
                        .collect();
                    receipt.l2_to_l1_logs = l2_to_l1_logs;

                    Ok(Some(receipt))
                }
                None => Ok(None),
            }
        }
    }

    pub async fn get_transaction(
        &mut self,
        transaction_id: api::TransactionId,
        chain_id: L2ChainId,
    ) -> Result<Option<api::Transaction>, SqlxError> {
        let where_sql = match transaction_id {
            api::TransactionId::Hash(_) => "transactions.hash = $1".to_owned(),
            api::TransactionId::Block(block_id, _) => {
                format!(
                    "transactions.index_in_block = $1 AND {}",
                    web3_block_where_sql(block_id, 2)
                )
            }
        };
        let query = format!(
            "SELECT {}
            FROM transactions
            LEFT JOIN miniblocks ON miniblocks.number = transactions.miniblock_number
            WHERE {where_sql}",
            web3_transaction_select_sql()
        );
        let query = sqlx::query(&query);

        let query = match &transaction_id {
            api::TransactionId::Hash(tx_hash) => query.bind(tx_hash.as_bytes()),
            api::TransactionId::Block(block_id, tx_index) => {
                let tx_index = if tx_index.as_u64() > i32::MAX as u64 {
                    return Ok(None);
                } else {
                    tx_index.as_u64() as i32
                };
                bind_block_where_sql_params(block_id, query.bind(tx_index))
            }
        };

        let tx = query
            .fetch_optional(self.storage.conn())
            .await?
            .map(|row| extract_web3_transaction(row, chain_id));
        Ok(tx)
    }

    pub async fn get_transaction_details(
        &mut self,
        hash: H256,
    ) -> Result<Option<api::TransactionDetails>, SqlxError> {
        {
            let storage_tx_details: Option<StorageTransactionDetails> = sqlx::query_as!(
                StorageTransactionDetails,
                r#"
                    SELECT transactions.is_priority,
                        transactions.initiator_address,
                        transactions.gas_limit,
                        transactions.gas_per_pubdata_limit,
                        transactions.received_at,
                        transactions.miniblock_number,
                        transactions.error,
                        transactions.effective_gas_price,
                        transactions.refunded_gas,
                        commit_tx.tx_hash as "eth_commit_tx_hash?",
                        prove_tx.tx_hash as "eth_prove_tx_hash?",
                        execute_tx.tx_hash as "eth_execute_tx_hash?"
                    FROM transactions
                    LEFT JOIN miniblocks ON miniblocks.number = transactions.miniblock_number
                    LEFT JOIN l1_batches ON l1_batches.number = miniblocks.l1_batch_number
                    LEFT JOIN eth_txs_history as commit_tx ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id AND commit_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs_history as prove_tx ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id AND prove_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs_history as execute_tx ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id AND execute_tx.confirmed_at IS NOT NULL)
                    WHERE transactions.hash = $1
                "#,
                hash.as_bytes()
            )
            .instrument("get_transaction_details")
            .with_arg("hash", &hash)
            .fetch_optional(self.storage.conn())
            .await?;

            let tx = storage_tx_details.map(|tx_details| tx_details.into());

            Ok(tx)
        }
    }

    /// Returns hashes of txs which were received after `from_timestamp` and the time of receiving the last tx.
    pub async fn get_pending_txs_hashes_after(
        &mut self,
        from_timestamp: NaiveDateTime,
        limit: Option<usize>,
    ) -> Result<(Vec<H256>, Option<NaiveDateTime>), SqlxError> {
        let records = sqlx::query!(
            "SELECT transactions.hash, transactions.received_at \
            FROM transactions \
            LEFT JOIN miniblocks ON miniblocks.number = miniblock_number \
            WHERE received_at > $1 \
            ORDER BY received_at ASC \
            LIMIT $2",
            from_timestamp,
            limit.map(|limit| limit as i64)
        )
        .fetch_all(self.storage.conn())
        .await?;

        let last_loc = records.last().map(|record| record.received_at);
        let hashes = records
            .into_iter()
            .map(|record| H256::from_slice(&record.hash))
            .collect();
        Ok((hashes, last_loc))
    }

    pub async fn next_nonce_by_initiator_account(
        &mut self,
        initiator_address: Address,
    ) -> Result<U256, SqlxError> {
        let latest_block_number = self
            .storage
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Latest))
            .await?
            .expect("Failed to get `latest` nonce");
        let latest_nonce = self
            .storage
            .storage_web3_dal()
            .get_address_historical_nonce(initiator_address, latest_block_number)
            .await?
            .as_u64();

        // Get nonces of non-rejected transactions, starting from the 'latest' nonce.
        // `latest` nonce is used, because it is guaranteed that there are no gaps before it.
        // `(miniblock_number IS NOT NULL OR error IS NULL)` is the condition that filters non-rejected transactions.
        // Query is fast because we have an index on (`initiator_address`, `nonce`)
        // and it cannot return more than `max_nonce_ahead` nonces.
        let non_rejected_nonces: Vec<u64> = sqlx::query!(
            "SELECT nonce as \"nonce!\" FROM transactions \
            WHERE initiator_address = $1 AND nonce >= $2 \
                AND is_priority = FALSE \
                AND (miniblock_number IS NOT NULL OR error IS NULL) \
            ORDER BY nonce",
            initiator_address.as_bytes(),
            latest_nonce as i64
        )
        .fetch_all(self.storage.conn())
        .await?
        .into_iter()
        .map(|row| row.nonce as u64)
        .collect();

        // Find pending nonce as the first "gap" in nonces.
        let mut pending_nonce = latest_nonce;
        for nonce in non_rejected_nonces {
            if pending_nonce == nonce {
                pending_nonce += 1;
            } else {
                break;
            }
        }

        Ok(U256::from(pending_nonce))
    }

    /// Returns the server transactions (not API ones) from a certain miniblock.
    /// Returns an empty list if the miniblock doesn't exist.
    pub async fn get_raw_miniblock_transactions(
        &mut self,
        miniblock: MiniblockNumber,
    ) -> Result<Vec<Transaction>, SqlxError> {
        let rows = sqlx::query_as!(
            StorageTransaction,
            "SELECT * FROM transactions \
            WHERE miniblock_number = $1 \
            ORDER BY index_in_block",
            miniblock.0 as i64
        )
        .fetch_all(self.storage.conn())
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{
        block::miniblock_hash, fee::TransactionExecutionMetrics, l2::L2Tx, ProtocolVersion,
    };

    use super::*;
    use crate::{
        tests::{create_miniblock_header, mock_execution_result, mock_l2_transaction},
        ConnectionPool,
    };

    async fn prepare_transaction(conn: &mut StorageProcessor<'_>, tx: L2Tx) {
        conn.blocks_dal()
            .delete_miniblocks(MiniblockNumber(0))
            .await
            .unwrap();
        conn.transactions_dal()
            .insert_transaction_l2(tx.clone(), TransactionExecutionMetrics::default())
            .await;
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(0))
            .await
            .unwrap();
        let mut miniblock_header = create_miniblock_header(1);
        miniblock_header.l2_tx_count = 1;
        conn.blocks_dal()
            .insert_miniblock(&miniblock_header)
            .await
            .unwrap();

        let tx_results = [mock_execution_result(tx)];
        conn.transactions_dal()
            .mark_txs_as_executed_in_miniblock(MiniblockNumber(1), &tx_results, U256::from(1))
            .await;
    }

    #[tokio::test]
    async fn getting_transaction() {
        let connection_pool = ConnectionPool::test_pool().await;
        let mut conn = connection_pool.access_storage().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        let tx = mock_l2_transaction();
        let tx_hash = tx.hash();
        prepare_transaction(&mut conn, tx).await;

        let block_ids = [
            api::BlockId::Number(api::BlockNumber::Latest),
            api::BlockId::Number(api::BlockNumber::Number(1.into())),
            api::BlockId::Hash(miniblock_hash(
                MiniblockNumber(1),
                0,
                H256::zero(),
                H256::zero(),
            )),
        ];
        let transaction_ids = block_ids
            .iter()
            .map(|&block_id| api::TransactionId::Block(block_id, 0.into()))
            .chain([api::TransactionId::Hash(tx_hash)]);

        for transaction_id in transaction_ids {
            let web3_tx = conn
                .transactions_web3_dal()
                .get_transaction(transaction_id, L2ChainId::from(270))
                .await;
            let web3_tx = web3_tx.unwrap().unwrap();
            assert_eq!(web3_tx.hash, tx_hash);
            assert_eq!(web3_tx.block_number, Some(1.into()));
            assert_eq!(web3_tx.transaction_index, Some(0.into()));
        }

        let transactions_with_bogus_index = block_ids
            .iter()
            .map(|&block_id| api::TransactionId::Block(block_id, 1.into()));
        for transaction_id in transactions_with_bogus_index {
            let web3_tx = conn
                .transactions_web3_dal()
                .get_transaction(transaction_id, L2ChainId::from(270))
                .await;
            assert!(web3_tx.unwrap().is_none());
        }

        let bogus_block_ids = [
            api::BlockId::Number(api::BlockNumber::Earliest),
            api::BlockId::Number(api::BlockNumber::Pending),
            api::BlockId::Number(api::BlockNumber::Number(42.into())),
            api::BlockId::Hash(H256::zero()),
        ];
        let transactions_with_bogus_block = bogus_block_ids
            .iter()
            .map(|&block_id| api::TransactionId::Block(block_id, 0.into()));
        for transaction_id in transactions_with_bogus_block {
            let web3_tx = conn
                .transactions_web3_dal()
                .get_transaction(transaction_id, L2ChainId::from(270))
                .await;
            assert!(web3_tx.unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn getting_miniblock_transactions() {
        let connection_pool = ConnectionPool::test_pool().await;
        let mut conn = connection_pool.access_storage().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        let tx = mock_l2_transaction();
        let tx_hash = tx.hash();
        prepare_transaction(&mut conn, tx).await;

        let raw_txs = conn
            .transactions_web3_dal()
            .get_raw_miniblock_transactions(MiniblockNumber(0))
            .await
            .unwrap();
        assert!(raw_txs.is_empty());

        let raw_txs = conn
            .transactions_web3_dal()
            .get_raw_miniblock_transactions(MiniblockNumber(1))
            .await
            .unwrap();
        assert_eq!(raw_txs.len(), 1);
        assert_eq!(raw_txs[0].hash(), tx_hash);
    }
}
