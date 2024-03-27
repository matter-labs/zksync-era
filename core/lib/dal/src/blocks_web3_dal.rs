use zksync_db_connection::{
    connection::Connection, instrument::InstrumentExt, interpolate_query, match_query_as,
};
use zksync_system_constants::EMPTY_UNCLES_HASH;
use zksync_types::{
    api,
    l2_to_l1_log::L2ToL1Log,
    vm_trace::Call,
    web3::types::{BlockHeader, U64},
    Bytes, L1BatchNumber, MiniblockNumber, H160, H2048, H256, U256,
};
use zksync_utils::bigdecimal_to_u256;

use crate::{
    models::{
        storage_block::{ResolvedL1BatchForMiniblock, StorageBlockDetails, StorageL1BatchDetails},
        storage_transaction::CallTrace,
    },
    Core, CoreDal,
};

const BLOCK_GAS_LIMIT: u32 = u32::MAX;

#[derive(Debug)]
pub struct BlocksWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl BlocksWeb3Dal<'_, '_> {
    pub async fn get_api_block(
        &mut self,
        block_number: MiniblockNumber,
    ) -> sqlx::Result<Option<api::Block<H256>>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                miniblocks.hash AS block_hash,
                miniblocks.number,
                miniblocks.l1_batch_number,
                miniblocks.timestamp,
                miniblocks.base_fee_per_gas,
                prev_miniblock.hash AS "parent_hash?",
                l1_batches.timestamp AS "l1_batch_timestamp?",
                transactions.gas_limit AS "gas_limit?",
                transactions.refunded_gas AS "refunded_gas?",
                transactions.hash AS "tx_hash?"
            FROM
                miniblocks
                LEFT JOIN miniblocks prev_miniblock ON prev_miniblock.number = miniblocks.number - 1
                LEFT JOIN l1_batches ON l1_batches.number = miniblocks.l1_batch_number
                LEFT JOIN transactions ON transactions.miniblock_number = miniblocks.number
            WHERE
                miniblocks.number = $1
            ORDER BY
                transactions.index_in_block ASC
            "#,
            i64::from(block_number.0)
        )
        .fetch_all(self.storage.conn())
        .await?;

        let block = rows.into_iter().fold(None, |prev_block, row| {
            let mut block = prev_block.unwrap_or_else(|| {
                // This code will be only executed for the first row in the DB response.
                // All other rows will only be used to extract relevant transactions.
                api::Block {
                    hash: H256::from_slice(&row.block_hash),
                    parent_hash: row
                        .parent_hash
                        .as_deref()
                        .map_or_else(H256::zero, H256::from_slice),
                    uncles_hash: EMPTY_UNCLES_HASH,
                    number: (row.number as u64).into(),
                    l1_batch_number: row.l1_batch_number.map(|number| (number as u64).into()),
                    gas_limit: BLOCK_GAS_LIMIT.into(),
                    base_fee_per_gas: bigdecimal_to_u256(row.base_fee_per_gas),
                    timestamp: (row.timestamp as u64).into(),
                    l1_batch_timestamp: row.l1_batch_timestamp.map(U256::from),
                    // TODO: include logs
                    ..api::Block::default()
                }
            });

            if let (Some(gas_limit), Some(refunded_gas)) = (row.gas_limit, row.refunded_gas) {
                block.gas_used += bigdecimal_to_u256(gas_limit) - U256::from(refunded_gas as u64);
            }
            if let Some(tx_hash) = &row.tx_hash {
                block.transactions.push(H256::from_slice(tx_hash));
            }
            Some(block)
        });

        Ok(block)
    }

    pub async fn get_block_tx_count(
        &mut self,
        block_number: MiniblockNumber,
    ) -> sqlx::Result<Option<u64>> {
        let tx_count = sqlx::query_scalar!(
            r#"
            SELECT l1_tx_count + l2_tx_count AS tx_count FROM miniblocks
            WHERE number = $1
            "#,
            i64::from(block_number.0)
        )
        .fetch_optional(self.storage.conn())
        .await?
        .flatten();

        Ok(tx_count.map(|count| count as u64))
    }

    /// Returns hashes of blocks with numbers starting from `from_block` and the number of the last block.
    pub async fn get_block_hashes_since(
        &mut self,
        from_block: MiniblockNumber,
        limit: usize,
    ) -> sqlx::Result<(Vec<H256>, Option<MiniblockNumber>)> {
        let rows = sqlx::query!(
            r#"
            SELECT
                number,
                hash
            FROM
                miniblocks
            WHERE
                number >= $1
            ORDER BY
                number ASC
            LIMIT
                $2
            "#,
            i64::from(from_block.0),
            limit as i32
        )
        .fetch_all(self.storage.conn())
        .await?;

        let last_block_number = rows.last().map(|row| MiniblockNumber(row.number as u32));
        let hashes = rows.iter().map(|row| H256::from_slice(&row.hash)).collect();
        Ok((hashes, last_block_number))
    }

    /// Returns hashes of blocks with numbers greater than `from_block` and the number of the last block.
    pub async fn get_block_headers_after(
        &mut self,
        from_block: MiniblockNumber,
    ) -> sqlx::Result<Vec<BlockHeader>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                hash,
                number,
                timestamp
            FROM
                miniblocks
            WHERE
                number > $1
            ORDER BY
                number ASC
            "#,
            i64::from(from_block.0),
        )
        .fetch_all(self.storage.conn())
        .await?;

        let blocks = rows.into_iter().map(|row| BlockHeader {
            hash: Some(H256::from_slice(&row.hash)),
            parent_hash: H256::zero(),
            uncles_hash: EMPTY_UNCLES_HASH,
            author: H160::zero(),
            state_root: H256::zero(),
            transactions_root: H256::zero(),
            receipts_root: H256::zero(),
            number: Some(U64::from(row.number)),
            gas_used: U256::zero(),
            gas_limit: U256::zero(),
            base_fee_per_gas: None,
            extra_data: Bytes::default(),
            // TODO: include logs
            logs_bloom: H2048::default(),
            timestamp: U256::from(row.timestamp),
            difficulty: U256::zero(),
            mix_hash: None,
            nonce: None,
        });
        Ok(blocks.collect())
    }

    pub async fn resolve_block_id(
        &mut self,
        block_id: api::BlockId,
    ) -> sqlx::Result<Option<MiniblockNumber>> {
        struct BlockNumberRow {
            number: Option<i64>,
        }

        let query = match_query_as!(
            BlockNumberRow,
            [_],
            match (block_id) {
                api::BlockId::Hash(hash) => ("SELECT number FROM miniblocks WHERE hash = $1"; hash.as_bytes()),
                api::BlockId::Number(api::BlockNumber::Number(number)) => (
                    "SELECT number FROM miniblocks WHERE number = $1";
                    number.as_u64() as i64
                ),
                api::BlockId::Number(api::BlockNumber::Earliest) => (
                    "SELECT number FROM miniblocks WHERE number = 0";
                ),
                api::BlockId::Number(api::BlockNumber::Pending) => (
                    "
                    SELECT COALESCE(
                        (SELECT (MAX(number) + 1) AS number FROM miniblocks),
                        (SELECT (MAX(miniblock_number) + 1) AS number FROM snapshot_recovery),
                        0
                    ) AS number
                    ";
                ),
                api::BlockId::Number(api::BlockNumber::Latest | api::BlockNumber::Committed) => (
                    "SELECT MAX(number) AS number FROM miniblocks";
                ),
                api::BlockId::Number(api::BlockNumber::Finalized) => (
                    "
                    SELECT COALESCE(
                        (
                            SELECT MAX(number) FROM miniblocks
                            WHERE l1_batch_number = (
                                SELECT MAX(number) FROM l1_batches
                                JOIN eth_txs ON
                                    l1_batches.eth_execute_tx_id = eth_txs.id
                                WHERE
                                    eth_txs.confirmed_eth_tx_history_id IS NOT NULL
                            )
                        ),
                        0
                    ) AS number
                    ";
                ),
            }
        );

        let row = query.fetch_optional(self.storage.conn()).await?;
        let block_number = row
            .and_then(|row| row.number)
            .map(|number| MiniblockNumber(number as u32));
        Ok(block_number)
    }

    /// Returns L1 batch timestamp for either sealed or pending L1 batch.
    ///
    /// The correctness of the current implementation depends on the timestamp of an L1 batch always
    /// being equal to the timestamp of the first miniblock in the batch.
    pub async fn get_expected_l1_batch_timestamp(
        &mut self,
        l1_batch_number: &ResolvedL1BatchForMiniblock,
    ) -> sqlx::Result<Option<u64>> {
        if let Some(miniblock_l1_batch) = l1_batch_number.miniblock_l1_batch {
            Ok(sqlx::query!(
                r#"
                SELECT
                    timestamp
                FROM
                    miniblocks
                WHERE
                    l1_batch_number = $1
                ORDER BY
                    number
                LIMIT
                    1
                "#,
                i64::from(miniblock_l1_batch.0)
            )
            .fetch_optional(self.storage.conn())
            .await?
            .map(|row| row.timestamp as u64))
        } else {
            // Got a pending miniblock. Searching the timestamp of the first pending miniblock using
            // `WHERE l1_batch_number IS NULL` is slow since it potentially locks the `miniblocks` table.
            // Instead, we determine its number using the previous L1 batch, taking into the account that
            // it may be stored in the `snapshot_recovery` table.
            let prev_l1_batch_number = if l1_batch_number.pending_l1_batch == L1BatchNumber(0) {
                return Ok(None); // We haven't created the genesis miniblock yet
            } else {
                l1_batch_number.pending_l1_batch - 1
            };
            Ok(sqlx::query!(
                r#"
                SELECT
                    timestamp
                FROM
                    miniblocks
                WHERE
                    number = COALESCE(
                        (
                            SELECT
                                MAX(number) + 1
                            FROM
                                miniblocks
                            WHERE
                                l1_batch_number = $1
                        ),
                        (
                            SELECT
                                MAX(miniblock_number) + 1
                            FROM
                                snapshot_recovery
                            WHERE
                                l1_batch_number = $1
                        )
                    )
                "#,
                i64::from(prev_l1_batch_number.0)
            )
            .fetch_optional(self.storage.conn())
            .await?
            .map(|row| row.timestamp as u64))
        }
    }

    pub async fn get_miniblock_hash(
        &mut self,
        block_number: MiniblockNumber,
    ) -> sqlx::Result<Option<H256>> {
        let hash = sqlx::query!(
            r#"
            SELECT
                hash
            FROM
                miniblocks
            WHERE
                number = $1
            "#,
            i64::from(block_number.0)
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| H256::from_slice(&row.hash));
        Ok(hash)
    }

    pub async fn get_l2_to_l1_logs(
        &mut self,
        block_number: L1BatchNumber,
    ) -> sqlx::Result<Vec<L2ToL1Log>> {
        let raw_logs = sqlx::query!(
            r#"
            SELECT
                l2_to_l1_logs
            FROM
                l1_batches
            WHERE
                number = $1
            "#,
            i64::from(block_number.0)
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.l2_to_l1_logs)
        .unwrap_or_default();

        Ok(raw_logs
            .into_iter()
            .map(|bytes| L2ToL1Log::from_slice(&bytes))
            .collect())
    }

    pub async fn get_l1_batch_number_of_miniblock(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> sqlx::Result<Option<L1BatchNumber>> {
        let number: Option<i64> = sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                miniblocks
            WHERE
                number = $1
            "#,
            i64::from(miniblock_number.0)
        )
        .fetch_optional(self.storage.conn())
        .await?
        .and_then(|row| row.l1_batch_number);

        Ok(number.map(|number| L1BatchNumber(number as u32)))
    }

    pub async fn get_miniblock_range_of_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<(MiniblockNumber, MiniblockNumber)>> {
        let row = sqlx::query!(
            r#"
            SELECT
                MIN(miniblocks.number) AS "min?",
                MAX(miniblocks.number) AS "max?"
            FROM
                miniblocks
            WHERE
                l1_batch_number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .fetch_one(self.storage.conn())
        .await?;

        Ok(match (row.min, row.max) {
            (Some(min), Some(max)) => {
                Some((MiniblockNumber(min as u32), MiniblockNumber(max as u32)))
            }
            (None, None) => None,
            _ => unreachable!(),
        })
    }

    pub async fn get_l1_batch_info_for_tx(
        &mut self,
        tx_hash: H256,
    ) -> sqlx::Result<Option<(L1BatchNumber, u16)>> {
        let row = sqlx::query!(
            r#"
            SELECT
                l1_batch_number,
                l1_batch_tx_index
            FROM
                transactions
            WHERE
                hash = $1
            "#,
            tx_hash.as_bytes()
        )
        .fetch_optional(self.storage.conn())
        .await?;

        let result = row.and_then(|row| match (row.l1_batch_number, row.l1_batch_tx_index) {
            (Some(l1_batch_number), Some(l1_batch_tx_index)) => Some((
                L1BatchNumber(l1_batch_number as u32),
                l1_batch_tx_index as u16,
            )),
            _ => None,
        });
        Ok(result)
    }

    /// Returns call traces for all transactions in the specified miniblock in the order of their execution.
    pub async fn get_traces_for_miniblock(
        &mut self,
        block_number: MiniblockNumber,
    ) -> sqlx::Result<Vec<Call>> {
        Ok(sqlx::query_as!(
            CallTrace,
            r#"
            SELECT
                call_trace
            FROM
                call_traces
                INNER JOIN transactions ON tx_hash = transactions.hash
            WHERE
                transactions.miniblock_number = $1
            ORDER BY
                transactions.index_in_block
            "#,
            i64::from(block_number.0)
        )
        .fetch_all(self.storage.conn())
        .await?
        .into_iter()
        .map(Call::from)
        .collect())
    }

    /// Returns `base_fee_per_gas` for miniblock range [min(newest_block - block_count + 1, 0), newest_block]
    /// in descending order of miniblock numbers.
    pub async fn get_fee_history(
        &mut self,
        newest_block: MiniblockNumber,
        block_count: u64,
    ) -> sqlx::Result<Vec<U256>> {
        let result: Vec<_> = sqlx::query!(
            r#"
            SELECT
                base_fee_per_gas
            FROM
                miniblocks
            WHERE
                number <= $1
            ORDER BY
                number DESC
            LIMIT
                $2
            "#,
            i64::from(newest_block.0),
            block_count as i64
        )
        .fetch_all(self.storage.conn())
        .await?
        .into_iter()
        .map(|row| bigdecimal_to_u256(row.base_fee_per_gas))
        .collect();

        Ok(result)
    }

    pub async fn get_block_details(
        &mut self,
        block_number: MiniblockNumber,
    ) -> sqlx::Result<Option<api::BlockDetails>> {
        let storage_block_details = sqlx::query_as!(
            StorageBlockDetails,
            r#"
            SELECT
                miniblocks.number,
                COALESCE(
                    miniblocks.l1_batch_number,
                    (
                        SELECT
                            (MAX(number) + 1)
                        FROM
                            l1_batches
                    )
                ) AS "l1_batch_number!",
                miniblocks.timestamp,
                miniblocks.l1_tx_count,
                miniblocks.l2_tx_count,
                miniblocks.hash AS "root_hash?",
                commit_tx.tx_hash AS "commit_tx_hash?",
                commit_tx.confirmed_at AS "committed_at?",
                prove_tx.tx_hash AS "prove_tx_hash?",
                prove_tx.confirmed_at AS "proven_at?",
                execute_tx.tx_hash AS "execute_tx_hash?",
                execute_tx.confirmed_at AS "executed_at?",
                miniblocks.l1_gas_price,
                miniblocks.l2_fair_gas_price,
                miniblocks.bootloader_code_hash,
                miniblocks.default_aa_code_hash,
                miniblocks.protocol_version,
                miniblocks.fee_account_address
            FROM
                miniblocks
                LEFT JOIN l1_batches ON miniblocks.l1_batch_number = l1_batches.number
                LEFT JOIN eth_txs_history AS commit_tx ON (
                    l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id
                    AND commit_tx.confirmed_at IS NOT NULL
                )
                LEFT JOIN eth_txs_history AS prove_tx ON (
                    l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id
                    AND prove_tx.confirmed_at IS NOT NULL
                )
                LEFT JOIN eth_txs_history AS execute_tx ON (
                    l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id
                    AND execute_tx.confirmed_at IS NOT NULL
                )
            WHERE
                miniblocks.number = $1
            "#,
            i64::from(block_number.0)
        )
        .instrument("get_block_details")
        .with_arg("block_number", &block_number)
        .report_latency()
        .fetch_optional(self.storage)
        .await?;

        let Some(storage_block_details) = storage_block_details else {
            return Ok(None);
        };
        let mut details = api::BlockDetails::from(storage_block_details);

        // FIXME (PLA-728): remove after 2nd phase of `fee_account_address` migration
        #[allow(deprecated)]
        self.storage
            .blocks_dal()
            .maybe_load_fee_address(&mut details.operator_address, details.number)
            .await?;
        Ok(Some(details))
    }

    pub async fn get_l1_batch_details(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<api::L1BatchDetails>> {
        let l1_batch_details: Option<StorageL1BatchDetails> = sqlx::query_as!(
            StorageL1BatchDetails,
            r#"
            WITH
                mb AS (
                    SELECT
                        l1_gas_price,
                        l2_fair_gas_price
                    FROM
                        miniblocks
                    WHERE
                        l1_batch_number = $1
                    LIMIT
                        1
                )
            SELECT
                l1_batches.number,
                l1_batches.timestamp,
                l1_batches.l1_tx_count,
                l1_batches.l2_tx_count,
                l1_batches.hash AS "root_hash?",
                commit_tx.tx_hash AS "commit_tx_hash?",
                commit_tx.confirmed_at AS "committed_at?",
                prove_tx.tx_hash AS "prove_tx_hash?",
                prove_tx.confirmed_at AS "proven_at?",
                execute_tx.tx_hash AS "execute_tx_hash?",
                execute_tx.confirmed_at AS "executed_at?",
                mb.l1_gas_price,
                mb.l2_fair_gas_price,
                l1_batches.bootloader_code_hash,
                l1_batches.default_aa_code_hash
            FROM
                l1_batches
                INNER JOIN mb ON TRUE
                LEFT JOIN eth_txs_history AS commit_tx ON (
                    l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id
                    AND commit_tx.confirmed_at IS NOT NULL
                )
                LEFT JOIN eth_txs_history AS prove_tx ON (
                    l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id
                    AND prove_tx.confirmed_at IS NOT NULL
                )
                LEFT JOIN eth_txs_history AS execute_tx ON (
                    l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id
                    AND execute_tx.confirmed_at IS NOT NULL
                )
            WHERE
                l1_batches.number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .instrument("get_l1_batch_details")
        .with_arg("l1_batch_number", &l1_batch_number)
        .report_latency()
        .fetch_optional(self.storage)
        .await?;

        Ok(l1_batch_details.map(Into::into))
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{
        block::{MiniblockHasher, MiniblockHeader},
        fee::TransactionExecutionMetrics,
        Address, MiniblockNumber, ProtocolVersion, ProtocolVersionId,
    };

    use super::*;
    use crate::{
        tests::{
            create_miniblock_header, create_snapshot_recovery, mock_execution_result,
            mock_l2_transaction,
        },
        ConnectionPool, Core,
    };

    #[tokio::test]
    async fn getting_web3_block_and_tx_count() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.blocks_dal()
            .delete_miniblocks(MiniblockNumber(0))
            .await
            .unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        let header = MiniblockHeader {
            l1_tx_count: 3,
            l2_tx_count: 5,
            ..create_miniblock_header(0)
        };
        conn.blocks_dal().insert_miniblock(&header).await.unwrap();

        let block_hash = MiniblockHasher::new(MiniblockNumber(0), 0, H256::zero())
            .finalize(ProtocolVersionId::latest());
        let block = conn
            .blocks_web3_dal()
            .get_api_block(MiniblockNumber(0))
            .await;
        let block = block.unwrap().unwrap();
        assert!(block.transactions.is_empty());
        assert_eq!(block.number, U64::zero());
        assert_eq!(block.hash, block_hash);

        let tx_count = conn
            .blocks_web3_dal()
            .get_block_tx_count(MiniblockNumber(0))
            .await;
        assert_eq!(tx_count.unwrap(), Some(8));

        let block = conn
            .blocks_web3_dal()
            .get_api_block(MiniblockNumber(1))
            .await;
        assert!(block.unwrap().is_none());

        let tx_count = conn
            .blocks_web3_dal()
            .get_block_tx_count(MiniblockNumber(1))
            .await;
        assert_eq!(tx_count.unwrap(), None);
    }

    #[tokio::test]
    async fn resolving_earliest_block_id() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();

        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Earliest))
            .await;
        assert_eq!(miniblock_number.unwrap(), None);

        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(0))
            .await
            .unwrap();

        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Earliest))
            .await;
        assert_eq!(miniblock_number.unwrap(), Some(MiniblockNumber(0)));
    }

    #[tokio::test]
    async fn resolving_latest_block_id() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;

        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Latest))
            .await
            .unwrap();
        assert_eq!(miniblock_number, None);
        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Pending))
            .await
            .unwrap();
        assert_eq!(miniblock_number, Some(MiniblockNumber(0)));

        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(0))
            .await
            .unwrap();

        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Latest))
            .await;
        assert_eq!(miniblock_number.unwrap(), Some(MiniblockNumber(0)));

        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Number(0.into())))
            .await;
        assert_eq!(miniblock_number.unwrap(), Some(MiniblockNumber(0)));
        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Number(1.into())))
            .await;
        assert_eq!(miniblock_number.unwrap(), None);

        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(1))
            .await
            .unwrap();
        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Latest))
            .await;
        assert_eq!(miniblock_number.unwrap(), Some(MiniblockNumber(1)));

        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Pending))
            .await;
        assert_eq!(miniblock_number.unwrap(), Some(MiniblockNumber(2)));

        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Number(1.into())))
            .await;
        assert_eq!(miniblock_number.unwrap(), Some(MiniblockNumber(1)));
    }

    #[tokio::test]
    async fn resolving_pending_block_id_for_snapshot_recovery() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        let snapshot_recovery = create_snapshot_recovery();
        conn.snapshot_recovery_dal()
            .insert_initial_recovery_status(&snapshot_recovery)
            .await
            .unwrap();

        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Pending))
            .await
            .unwrap();
        assert_eq!(miniblock_number, Some(MiniblockNumber(43)));
    }

    #[tokio::test]
    async fn resolving_block_by_hash() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(0))
            .await
            .unwrap();

        let hash = MiniblockHasher::new(MiniblockNumber(0), 0, H256::zero())
            .finalize(ProtocolVersionId::latest());
        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Hash(hash))
            .await;
        assert_eq!(miniblock_number.unwrap(), Some(MiniblockNumber(0)));

        let hash = MiniblockHasher::new(MiniblockNumber(1), 1, H256::zero())
            .finalize(ProtocolVersionId::latest());
        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Hash(hash))
            .await;
        assert_eq!(miniblock_number.unwrap(), None);
    }

    #[tokio::test]
    async fn getting_traces_for_block() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(1))
            .await
            .unwrap();

        let transactions = [mock_l2_transaction(), mock_l2_transaction()];
        let mut tx_results = vec![];
        for (i, tx) in transactions.into_iter().enumerate() {
            conn.transactions_dal()
                .insert_transaction_l2(tx.clone(), TransactionExecutionMetrics::default())
                .await
                .unwrap();
            let mut tx_result = mock_execution_result(tx);
            tx_result.call_traces.push(Call {
                from: Address::from_low_u64_be(i as u64),
                to: Address::from_low_u64_be(i as u64 + 1),
                value: i.into(),
                ..Call::default()
            });
            tx_results.push(tx_result);
        }
        conn.transactions_dal()
            .mark_txs_as_executed_in_miniblock(MiniblockNumber(1), &tx_results, 1.into())
            .await;

        let traces = conn
            .blocks_web3_dal()
            .get_traces_for_miniblock(MiniblockNumber(1))
            .await
            .unwrap();
        assert_eq!(traces.len(), 2);
        for (trace, tx_result) in traces.iter().zip(&tx_results) {
            let expected_trace = tx_result.call_trace().unwrap();
            assert_eq!(*trace, expected_trace);
        }
    }
}
