use std::{collections::HashMap, iter::once};

use anyhow::Context as _;
use sqlx::types::chrono::NaiveDateTime;
use zksync_db_connection::{
    connection::Connection,
    error::{DalResult, SqlxContext as _},
    instrument::InstrumentExt,
    interpolate_query, match_query_as,
};
use zksync_types::{
    api, api::TransactionReceipt, block::build_bloom, web3, Address, BloomInput, L2BlockNumber,
    L2ChainId, Transaction, H256, U256,
};

use crate::{
    models::storage_transaction::{
        StorageApiTransaction, StorageTransaction, StorageTransactionDetails,
        StorageTransactionExecutionInfo, StorageTransactionReceipt,
    },
    Core, CoreDal,
};

#[derive(Debug, Clone, Copy)]
enum TransactionSelector<'a> {
    Hashes(&'a [H256]),
    Position(L2BlockNumber, u32),
}

/// Transaction receipt together with additional data used by the API server logic.
#[derive(Debug)]
pub struct ExtendedTransactionReceipt {
    pub inner: TransactionReceipt,
    pub nonce: U256,
    pub calldata: web3::Bytes,
}

#[derive(Debug)]
pub struct TransactionsWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl TransactionsWeb3Dal<'_, '_> {
    /// Returns receipts by transactions hashes. Hashes are expected to be unique.
    ///
    /// # Important!
    ///
    /// The returned receipts do not have `contract_address` set; this field needs to be filled separately.
    pub async fn get_transaction_receipts(
        &mut self,
        hashes: &[H256],
    ) -> DalResult<Vec<ExtendedTransactionReceipt>> {
        let hash_bytes: Vec<_> = hashes.iter().map(H256::as_bytes).collect();

        let st_receipts: Vec<StorageTransactionReceipt> = sqlx::query_as!(
            StorageTransactionReceipt,
            r#"
            SELECT
                transactions.hash AS tx_hash,
                transactions.index_in_block,
                transactions.l1_batch_tx_index,
                transactions.miniblock_number AS "block_number!",
                transactions.error,
                transactions.effective_gas_price,
                transactions.initiator_address,
                transactions.data -> 'to' AS "transfer_to?",
                transactions.data -> 'contractAddress' AS "execute_contract_address?",
                transactions.data -> 'calldata' AS "calldata",
                transactions.tx_format AS "tx_format?",
                transactions.refunded_gas,
                transactions.gas_limit,
                transactions.nonce,
                miniblocks.hash AS "block_hash",
                miniblocks.l1_batch_number AS "l1_batch_number?",
                miniblocks.timestamp AS "block_timestamp?"
            FROM
                transactions
            JOIN miniblocks ON miniblocks.number = transactions.miniblock_number
            WHERE
                transactions.hash = ANY($1)
                AND transactions.data != '{}'::jsonb
            "#,
            // ^ Filter out transactions with pruned data, which would lead to potentially incomplete / bogus
            // transaction info.
            &hash_bytes as &[&[u8]],
        )
        .instrument("get_transaction_receipts")
        .with_arg("hashes.len", &hashes.len())
        .fetch_all(self.storage)
        .await?;

        let block_timestamps: Vec<Option<i64>> =
            st_receipts.iter().map(|x| x.block_timestamp).collect();

        let mut receipts: Vec<ExtendedTransactionReceipt> =
            st_receipts.into_iter().map(Into::into).collect();

        let mut logs = self
            .storage
            .events_dal()
            .get_logs_by_tx_hashes(hashes)
            .await?;

        let mut l2_to_l1_logs = self
            .storage
            .events_dal()
            .get_l2_to_l1_logs_by_hashes(hashes)
            .await?;

        for (receipt, block_timestamp) in receipts.iter_mut().zip(block_timestamps.into_iter()) {
            let receipt = &mut receipt.inner;
            let logs_for_tx = logs.remove(&receipt.transaction_hash);

            if let Some(logs) = logs_for_tx {
                let iter = logs.iter().flat_map(|log| {
                    log.topics
                        .iter()
                        .map(|topic| BloomInput::Raw(topic.as_bytes()))
                        .chain(once(BloomInput::Raw(log.address.as_bytes())))
                });
                receipt.logs_bloom = build_bloom(iter);
                receipt.logs = logs
                    .into_iter()
                    .map(|mut log| {
                        log.block_hash = Some(receipt.block_hash);
                        log.l1_batch_number = receipt.l1_batch_number;
                        log.block_timestamp = block_timestamp.map(|t| (t as u64).into());
                        log
                    })
                    .collect();
            }

            let l2_to_l1_logs_for_tx = l2_to_l1_logs.remove(&receipt.transaction_hash);
            if let Some(l2_to_l1_logs) = l2_to_l1_logs_for_tx {
                receipt.l2_to_l1_logs = l2_to_l1_logs
                    .into_iter()
                    .map(|mut log| {
                        log.block_hash = Some(receipt.block_hash);
                        log.l1_batch_number = receipt.l1_batch_number;
                        log
                    })
                    .collect();
            }
        }

        Ok(receipts)
    }

    /// Obtains transactions with the specified hashes. Transactions are returned in no particular order; if some hashes
    /// don't correspond to transactions, the output will contain less elements than `hashes`.
    pub async fn get_transactions(
        &mut self,
        hashes: &[H256],
        chain_id: L2ChainId,
    ) -> DalResult<Vec<api::Transaction>> {
        self.get_transactions_inner(TransactionSelector::Hashes(hashes), chain_id)
            .await
    }

    pub async fn get_unstable_transaction_execution_info(
        &mut self,
        hash: H256,
    ) -> DalResult<Option<serde_json::Value>> {
        let row = sqlx::query_as!(
            StorageTransactionExecutionInfo,
            r#"
            SELECT
                transactions.execution_info
            FROM
                transactions
            WHERE
                transactions.hash = $1
            "#,
            hash.as_bytes()
        )
        .instrument("get_unstable_transaction_execution_info")
        .with_arg("hash", &hash)
        .fetch_optional(self.storage)
        .await?;
        Ok(row.map(|entry| entry.execution_info))
    }

    async fn get_transactions_inner(
        &mut self,
        selector: TransactionSelector<'_>,
        chain_id: L2ChainId,
    ) -> DalResult<Vec<api::Transaction>> {
        if let TransactionSelector::Position(_, idx) = selector {
            // Since index is not trusted, we check it to prevent potential overflow below.
            if idx > i32::MAX as u32 {
                return Ok(vec![]);
            }
        }

        let query = match_query_as!(
            StorageApiTransaction,
            [
                r#"
                SELECT
                    transactions.hash AS tx_hash,
                    transactions.index_in_block AS index_in_block,
                    miniblocks.number AS block_number,
                    transactions.nonce AS nonce,
                    transactions.signature AS signature,
                    transactions.initiator_address AS initiator_address,
                    transactions.tx_format AS tx_format,
                    transactions.value AS value,
                    transactions.gas_limit AS gas_limit,
                    transactions.max_fee_per_gas AS max_fee_per_gas,
                    transactions.max_priority_fee_per_gas AS max_priority_fee_per_gas,
                    transactions.effective_gas_price AS effective_gas_price,
                    transactions.l1_batch_number AS l1_batch_number,
                    transactions.l1_batch_tx_index AS l1_batch_tx_index,
                    transactions.data->'contractAddress' AS "execute_contract_address",
                    transactions.data->'calldata' AS "calldata",
                    miniblocks.hash AS "block_hash"
                FROM transactions
                LEFT JOIN miniblocks ON miniblocks.number = transactions.miniblock_number
                WHERE
                "#,
                _, // WHERE condition
                " AND transactions.data != '{}'::jsonb"
                // ^ Filter out transactions with pruned data, which would lead to potentially incomplete / bogus
                // transaction info.
            ],
            match (selector) {
                TransactionSelector::Hashes(hashes) => (
                    "transactions.hash = ANY($1)";
                    &hashes.iter().map(H256::as_bytes).collect::<Vec<_>>() as &[&[u8]]
                ),
                TransactionSelector::Position(block_number, idx) => (
                    "miniblocks.number = $1 AND transactions.index_in_block = $2";
                    i64::from(block_number.0),
                    idx as i32
                ),
            }
        );

        let rows = query
            .instrument("get_transactions")
            .with_arg("selector", &selector)
            .fetch_all(self.storage)
            .await?;
        Ok(rows.into_iter().map(|row| row.into_api(chain_id)).collect())
    }

    pub async fn get_transaction_by_hash(
        &mut self,
        hash: H256,
        chain_id: L2ChainId,
    ) -> DalResult<Option<api::Transaction>> {
        Ok(self
            .get_transactions_inner(TransactionSelector::Hashes(&[hash]), chain_id)
            .await?
            .into_iter()
            .next())
    }

    pub async fn get_transaction_by_position(
        &mut self,
        block_number: L2BlockNumber,
        index_in_block: u32,
        chain_id: L2ChainId,
    ) -> DalResult<Option<api::Transaction>> {
        Ok(self
            .get_transactions_inner(
                TransactionSelector::Position(block_number, index_in_block),
                chain_id,
            )
            .await?
            .into_iter()
            .next())
    }

    pub async fn get_transaction_details(
        &mut self,
        hash: H256,
    ) -> DalResult<Option<api::TransactionDetails>> {
        let row = sqlx::query_as!(
            StorageTransactionDetails,
            r#"
            SELECT
                transactions.is_priority,
                transactions.initiator_address,
                transactions.gas_limit,
                transactions.gas_per_pubdata_limit,
                transactions.received_at,
                miniblocks.number AS "miniblock_number?",
                transactions.error,
                transactions.effective_gas_price,
                transactions.refunded_gas,
                commit_tx.tx_hash AS "eth_commit_tx_hash?",
                prove_tx.tx_hash AS "eth_prove_tx_hash?",
                execute_tx.tx_hash AS "eth_execute_tx_hash?",
                execute_tx.finality_status AS "eth_execute_tx_finality_status?"
            FROM
                transactions
            LEFT JOIN miniblocks ON miniblocks.number = transactions.miniblock_number
            LEFT JOIN l1_batches ON l1_batches.number = miniblocks.l1_batch_number
            LEFT JOIN eth_txs_history AS commit_tx
                ON (
                    l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id
                    AND commit_tx.confirmed_at IS NOT NULL
                )
            LEFT JOIN eth_txs_history AS prove_tx
                ON (
                    l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id
                    AND prove_tx.confirmed_at IS NOT NULL
                )
            LEFT JOIN eth_txs_history AS execute_tx
                ON (
                    l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id
                    AND execute_tx.confirmed_at IS NOT NULL
                )
            WHERE
                transactions.hash = $1
                AND transactions.data != '{}'::jsonb
            "#,
            // ^ Filter out transactions with pruned data, which would lead to potentially incomplete / bogus
            // transaction info.
            hash.as_bytes()
        )
        .instrument("get_transaction_details")
        .with_arg("hash", &hash)
        .fetch_optional(self.storage)
        .await?;

        Ok(row.map(Into::into))
    }

    /// Returns hashes of txs which were received after `from_timestamp` and the time of receiving the last tx.
    pub async fn get_pending_txs_hashes_after(
        &mut self,
        from_timestamp: NaiveDateTime,
        limit: Option<usize>,
    ) -> DalResult<Vec<(NaiveDateTime, H256)>> {
        let records = sqlx::query!(
            r#"
            SELECT
                transactions.hash,
                transactions.received_at
            FROM
                transactions
            WHERE
                received_at > $1
            ORDER BY
                received_at ASC
            LIMIT
                $2
            "#,
            from_timestamp,
            limit.map(|limit| limit as i64)
        )
        .instrument("get_pending_txs_hashes_after")
        .with_arg("from_timestamp", &from_timestamp)
        .with_arg("limit", &limit)
        .fetch_all(self.storage)
        .await?;

        let hashes = records
            .into_iter()
            .map(|record| (record.received_at, H256::from_slice(&record.hash)))
            .collect();
        Ok(hashes)
    }

    /// `committed_next_nonce` should equal the nonce for `initiator_address` in the storage.
    pub async fn next_nonce_by_initiator_account(
        &mut self,
        initiator_address: Address,
        committed_next_nonce: u64,
    ) -> DalResult<U256> {
        // Get nonces of non-rejected transactions, starting from the 'latest' nonce.
        // `latest` nonce is used, because it is guaranteed that there are no gaps before it.
        // `(miniblock_number IS NOT NULL OR error IS NULL)` is the condition that filters non-rejected transactions.
        // Query is fast because we have an index on (`initiator_address`, `nonce`)
        // and it cannot return more than `max_nonce_ahead` nonces.
        let non_rejected_nonces: Vec<u64> = sqlx::query!(
            r#"
            SELECT
                nonce AS "nonce!"
            FROM
                transactions
            WHERE
                initiator_address = $1
                AND nonce >= $2
                AND is_priority = FALSE
                AND (
                    miniblock_number IS NOT NULL
                    OR error IS NULL
                )
            ORDER BY
                nonce
            "#,
            initiator_address.as_bytes(),
            committed_next_nonce as i64
        )
        .instrument("next_nonce_by_initiator_account#non_rejected_nonces")
        .with_arg("initiator_address", &initiator_address)
        .with_arg("committed_next_nonce", &committed_next_nonce)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|row| row.nonce as u64)
        .collect();

        // Find pending nonce as the first "gap" in nonces.
        let mut pending_nonce = committed_next_nonce;
        for nonce in non_rejected_nonces {
            if pending_nonce == nonce {
                pending_nonce += 1;
            } else {
                break;
            }
        }

        Ok(U256::from(pending_nonce))
    }

    /// Returns the server transactions (not API ones) from a L2 block range.
    pub async fn get_raw_l2_blocks_transactions(
        &mut self,
        blocks: std::ops::Range<L2BlockNumber>,
    ) -> DalResult<HashMap<L2BlockNumber, Vec<Transaction>>> {
        // Check if range is non-empty, because BETWEEN in SQL in `unordered`.
        if blocks.is_empty() {
            return Ok(HashMap::default());
        }
        // We do an inner join with `miniblocks.number`, because
        // transaction insertions are not atomic with miniblock insertion.
        let rows = sqlx::query_as!(
            StorageTransaction,
            r#"
            SELECT
                transactions.*
            FROM
                transactions
            INNER JOIN miniblocks ON miniblocks.number = transactions.miniblock_number
            WHERE
                miniblocks.number BETWEEN $1 AND $2
            ORDER BY
                miniblock_number,
                index_in_block
            "#,
            i64::from(blocks.start.0),
            i64::from(blocks.end.0 - 1),
        )
        .try_map(|row| {
            let to_block_number = |n: Option<i64>| {
                anyhow::Ok(L2BlockNumber(
                    n.context("missing")?.try_into().context("overflow")?,
                ))
            };
            Ok((
                to_block_number(row.miniblock_number).decode_column("miniblock_number")?,
                Transaction::from(row),
            ))
        })
        .instrument("get_raw_l2_blocks_transactions")
        .with_arg("blocks", &blocks)
        .fetch_all(self.storage)
        .await?;
        let mut txs: HashMap<L2BlockNumber, Vec<Transaction>> = HashMap::new();
        for (n, tx) in rows {
            txs.entry(n).or_default().push(tx);
        }
        Ok(txs)
    }

    /// Returns the server transactions (not API ones) from an L2 block.
    pub async fn get_raw_l2_block_transactions(
        &mut self,
        block: L2BlockNumber,
    ) -> DalResult<Vec<Transaction>> {
        Ok(self
            .get_raw_l2_blocks_transactions(block..block + 1)
            .await?
            .remove(&block)
            .unwrap_or_default())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use zksync_types::{l2::L2Tx, Nonce, ProtocolVersion, ProtocolVersionId};
    use zksync_vm_interface::{tracer::ValidationTraces, TransactionExecutionMetrics};

    use super::*;
    use crate::{
        tests::{create_l2_block_header, mock_execution_result, mock_l2_transaction},
        ConnectionPool, Core, CoreDal,
    };

    async fn prepare_transactions(conn: &mut Connection<'_, Core>, txs: Vec<L2Tx>) {
        conn.blocks_dal()
            .delete_l2_blocks(L2BlockNumber(0))
            .await
            .unwrap();

        for tx in &txs {
            conn.transactions_dal()
                .insert_transaction_l2(
                    tx,
                    TransactionExecutionMetrics::default(),
                    ValidationTraces::default(),
                )
                .await
                .unwrap();
        }
        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(0))
            .await
            .unwrap();
        let mut l2_block_header = create_l2_block_header(1);
        l2_block_header.l2_tx_count = txs.len() as u16;
        conn.blocks_dal()
            .insert_l2_block(&l2_block_header)
            .await
            .unwrap();

        let tx_results = txs
            .into_iter()
            .map(mock_execution_result)
            .collect::<Vec<_>>();

        conn.transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                L2BlockNumber(1),
                &tx_results,
                U256::from(1),
                ProtocolVersionId::latest(),
                false,
            )
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn getting_transaction() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        let tx = mock_l2_transaction();
        let tx_hash = tx.hash();
        prepare_transactions(&mut conn, vec![tx]).await;

        let web3_tx = conn
            .transactions_web3_dal()
            .get_transaction_by_position(L2BlockNumber(1), 0, L2ChainId::from(270))
            .await;
        let web3_tx = web3_tx.unwrap().unwrap();
        assert_eq!(web3_tx.hash, tx_hash);
        assert_eq!(web3_tx.block_number, Some(1.into()));
        assert_eq!(web3_tx.transaction_index, Some(0.into()));

        let web3_tx = conn
            .transactions_web3_dal()
            .get_transaction_by_hash(tx_hash, L2ChainId::from(270))
            .await;
        let web3_tx = web3_tx.unwrap().unwrap();
        assert_eq!(web3_tx.hash, tx_hash);
        assert_eq!(web3_tx.block_number, Some(1.into()));
        assert_eq!(web3_tx.transaction_index, Some(0.into()));

        for block_number in [0, 2, 100] {
            let web3_tx = conn
                .transactions_web3_dal()
                .get_transaction_by_position(L2BlockNumber(block_number), 0, L2ChainId::from(270))
                .await;
            assert!(web3_tx.unwrap().is_none());
        }
        for index in [1, 2, 100] {
            let web3_tx = conn
                .transactions_web3_dal()
                .get_transaction_by_position(L2BlockNumber(1), index, L2ChainId::from(270))
                .await;
            assert!(web3_tx.unwrap().is_none());
        }
        let web3_tx = conn
            .transactions_web3_dal()
            .get_transaction_by_hash(H256::zero(), L2ChainId::from(270))
            .await;
        assert!(web3_tx.unwrap().is_none());

        let execution_info = conn
            .transactions_web3_dal()
            .get_unstable_transaction_execution_info(tx_hash)
            .await
            .unwrap()
            .expect("Transaction execution info is missing in the DAL");

        // Check that execution info has at least the circuit statistics field.
        // If this assertion fails because the transaction execution info format
        // has changed, replace circuit_statistic with any other valid field
        assert!(
            execution_info.get("circuit_statistic").is_some(),
            "Missing circuit_statistics field"
        );
    }

    #[tokio::test]
    async fn getting_evm_deployment_tx() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        let mut tx = mock_l2_transaction();
        tx.execute.contract_address = None;
        let tx_hash = tx.hash();
        prepare_transactions(&mut conn, vec![tx.clone()]).await;

        let fetched_tx = conn
            .transactions_dal()
            .get_tx_by_hash(tx_hash)
            .await
            .unwrap()
            .expect("no transaction");
        let mut fetched_tx = L2Tx::try_from(fetched_tx).unwrap();
        assert_eq!(fetched_tx.execute.contract_address, None);
        fetched_tx.received_timestamp_ms = tx.received_timestamp_ms;
        fetched_tx.raw_bytes = tx.raw_bytes.clone();
        assert_eq!(fetched_tx, tx);

        let web3_tx = conn
            .transactions_web3_dal()
            .get_transaction_by_position(L2BlockNumber(1), 0, L2ChainId::from(270))
            .await;
        let web3_tx = web3_tx.unwrap().expect("no transaction");
        assert_eq!(web3_tx.hash, tx_hash);
        assert_eq!(web3_tx.to, None);
    }

    #[tokio::test]
    async fn getting_receipts() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let tx1 = mock_l2_transaction();
        let tx1_hash = tx1.hash();
        let tx2 = mock_l2_transaction();
        let tx2_hash = tx2.hash();

        prepare_transactions(&mut conn, vec![tx1, tx2]).await;

        let mut receipts = conn
            .transactions_web3_dal()
            .get_transaction_receipts(&[tx1_hash, tx2_hash])
            .await
            .unwrap();

        receipts.sort_unstable_by_key(|receipt| receipt.inner.transaction_index);

        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].inner.transaction_hash, tx1_hash);
        assert_eq!(receipts[1].inner.transaction_hash, tx2_hash);
    }

    #[tokio::test]
    async fn getting_receipt_for_evm_deployment_tx() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let mut tx = mock_l2_transaction();
        let tx_hash = tx.hash();
        tx.execute.contract_address = None;
        prepare_transactions(&mut conn, vec![tx]).await;

        let receipts = conn
            .transactions_web3_dal()
            .get_transaction_receipts(&[tx_hash])
            .await
            .unwrap();
        assert_eq!(receipts.len(), 1);
        let receipt = receipts.into_iter().next().unwrap().inner;
        assert_eq!(receipt.transaction_hash, tx_hash);
        assert_eq!(receipt.to, None);
    }

    #[tokio::test]
    async fn getting_l2_block_transactions() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        let tx = mock_l2_transaction();
        let tx_hash = tx.hash();
        prepare_transactions(&mut conn, vec![tx]).await;

        let raw_txs = conn
            .transactions_web3_dal()
            .get_raw_l2_block_transactions(L2BlockNumber(0))
            .await
            .unwrap();
        assert!(raw_txs.is_empty());

        let raw_txs = conn
            .transactions_web3_dal()
            .get_raw_l2_block_transactions(L2BlockNumber(1))
            .await
            .unwrap();
        assert_eq!(raw_txs.len(), 1);
        assert_eq!(raw_txs[0].hash(), tx_hash);
    }

    #[tokio::test]
    async fn getting_next_nonce_by_initiator_account() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let initiator = Address::repeat_byte(1);
        let next_nonce = conn
            .transactions_web3_dal()
            .next_nonce_by_initiator_account(initiator, 0)
            .await
            .unwrap();
        assert_eq!(next_nonce, 0.into());

        let mut tx_by_nonce = HashMap::new();
        for nonce in [0, 1, 4] {
            let mut tx = mock_l2_transaction();
            // Changing transaction fields invalidates its signature, but it's OK for test purposes
            tx.common_data.nonce = Nonce(nonce);
            tx.common_data.initiator_address = initiator;
            tx_by_nonce.insert(nonce, tx.clone());
            conn.transactions_dal()
                .insert_transaction_l2(
                    &tx,
                    TransactionExecutionMetrics::default(),
                    ValidationTraces::default(),
                )
                .await
                .unwrap();
        }

        let next_nonce = conn
            .transactions_web3_dal()
            .next_nonce_by_initiator_account(initiator, 0)
            .await
            .unwrap();
        assert_eq!(next_nonce, 2.into());

        // Reject the transaction with nonce 1, so that it'd be not taken into account.
        conn.transactions_dal()
            .mark_tx_as_rejected(tx_by_nonce[&1].hash(), "oops")
            .await
            .unwrap();
        let next_nonce = conn
            .transactions_web3_dal()
            .next_nonce_by_initiator_account(initiator, 0)
            .await
            .unwrap();
        assert_eq!(next_nonce, 1.into());

        // Include transactions in a L2 block (including the rejected one), so that they are taken into account again.
        let mut l2_block = create_l2_block_header(1);
        l2_block.l2_tx_count = 2;
        conn.blocks_dal().insert_l2_block(&l2_block).await.unwrap();
        let executed_txs = [
            mock_execution_result(tx_by_nonce[&0].clone()),
            mock_execution_result(tx_by_nonce[&1].clone()),
        ];
        conn.transactions_dal()
            .mark_txs_as_executed_in_l2_block(
                l2_block.number,
                &executed_txs,
                1.into(),
                ProtocolVersionId::latest(),
                false,
            )
            .await
            .unwrap();

        let next_nonce = conn
            .transactions_web3_dal()
            .next_nonce_by_initiator_account(initiator, 0)
            .await
            .unwrap();
        assert_eq!(next_nonce, 2.into());
    }

    #[tokio::test]
    async fn getting_next_nonce_by_initiator_account_after_snapshot_recovery() {
        // Emulate snapshot recovery: no transactions with past nonces are present in the storage
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        let initiator = Address::repeat_byte(1);
        let next_nonce = conn
            .transactions_web3_dal()
            .next_nonce_by_initiator_account(initiator, 1)
            .await
            .unwrap();
        assert_eq!(next_nonce, 1.into());

        let mut tx = mock_l2_transaction();
        // Changing transaction fields invalidates its signature, but it's OK for test purposes
        tx.common_data.nonce = Nonce(1);
        tx.common_data.initiator_address = initiator;
        conn.transactions_dal()
            .insert_transaction_l2(
                &tx,
                TransactionExecutionMetrics::default(),
                ValidationTraces::default(),
            )
            .await
            .unwrap();

        let next_nonce = conn
            .transactions_web3_dal()
            .next_nonce_by_initiator_account(initiator, 1)
            .await
            .unwrap();
        assert_eq!(next_nonce, 2.into());
    }
}
