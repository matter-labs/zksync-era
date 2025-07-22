use zksync_db_connection::{
    connection::Connection, error::DalResult, instrument::InstrumentExt, interpolate_query,
    match_query_as,
};
use zksync_system_constants::EMPTY_UNCLES_HASH;
use zksync_types::{
    api,
    debug_flat_call::CallTraceMeta,
    fee_model::BatchFeeInput,
    l2_to_l1_log::L2ToL1Log,
    web3::{BlockHeader, Bytes},
    Bloom, L1BatchNumber, L2BlockNumber, ProtocolVersionId, H160, H256, U256, U64,
};
use zksync_vm_interface::Call;

use crate::{
    models::{
        bigdecimal_to_u256, parse_protocol_version,
        storage_block::{
            ResolvedL1BatchForL2Block, StorageBlockDetails, StorageL1BatchDetails,
            LEGACY_BLOCK_GAS_LIMIT,
        },
        storage_transaction::CallTrace,
    },
    Core, CoreDal,
};

#[derive(Debug)]
pub struct BlocksWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl BlocksWeb3Dal<'_, '_> {
    pub async fn get_api_block(
        &mut self,
        block_number: L2BlockNumber,
    ) -> DalResult<Option<api::Block<H256>>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                miniblocks.hash AS block_hash,
                miniblocks.number,
                miniblocks.l1_batch_number,
                miniblocks.timestamp,
                miniblocks.base_fee_per_gas,
                miniblocks.gas_limit AS "block_gas_limit?",
                miniblocks.logs_bloom,
                prev_miniblock.hash AS "parent_hash?",
                l1_batches.timestamp AS "l1_batch_timestamp?",
                transactions.gas_limit AS "transaction_gas_limit?",
                transactions.refunded_gas AS "refunded_gas?",
                transactions.hash AS "tx_hash?"
            FROM
                miniblocks
            LEFT JOIN
                miniblocks prev_miniblock
                ON prev_miniblock.number = miniblocks.number - 1
            LEFT JOIN l1_batches ON l1_batches.number = miniblocks.l1_batch_number
            LEFT JOIN transactions ON transactions.miniblock_number = miniblocks.number
            WHERE
                miniblocks.number = $1
            ORDER BY
                transactions.index_in_block ASC
            "#,
            i64::from(block_number.0)
        )
        .instrument("get_api_block")
        .with_arg("block_number", &block_number)
        .fetch_all(self.storage)
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
                    base_fee_per_gas: bigdecimal_to_u256(row.base_fee_per_gas),
                    timestamp: (row.timestamp as u64).into(),
                    l1_batch_timestamp: row.l1_batch_timestamp.map(U256::from),
                    gas_limit: (row
                        .block_gas_limit
                        .unwrap_or(i64::from(LEGACY_BLOCK_GAS_LIMIT))
                        as u64)
                        .into(),
                    logs_bloom: row
                        .logs_bloom
                        .map(|b| Bloom::from_slice(&b))
                        .unwrap_or_default(),
                    ..api::Block::default()
                }
            });

            if let (Some(gas_limit), Some(refunded_gas)) =
                (row.transaction_gas_limit, row.refunded_gas)
            {
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
        block_number: L2BlockNumber,
    ) -> DalResult<Option<u64>> {
        let tx_count = sqlx::query_scalar!(
            r#"
            SELECT l1_tx_count + l2_tx_count AS tx_count FROM miniblocks
            WHERE number = $1
            "#,
            i64::from(block_number.0)
        )
        .instrument("get_block_tx_count")
        .with_arg("block_number", &block_number)
        .fetch_optional(self.storage)
        .await?
        .flatten();

        Ok(tx_count.map(|count| count as u64))
    }

    /// Returns hashes of blocks with numbers starting from `from_block` and the number of the last block.
    pub async fn get_block_hashes_since(
        &mut self,
        from_block: L2BlockNumber,
        limit: usize,
    ) -> DalResult<(Vec<H256>, Option<L2BlockNumber>)> {
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
        .instrument("get_block_hashes_since")
        .with_arg("from_block", &from_block)
        .with_arg("limit", &limit)
        .fetch_all(self.storage)
        .await?;

        let last_block_number = rows.last().map(|row| L2BlockNumber(row.number as u32));
        let hashes = rows.iter().map(|row| H256::from_slice(&row.hash)).collect();
        Ok((hashes, last_block_number))
    }

    /// Returns hashes of blocks with numbers greater than `from_block` and the number of the last block.
    pub async fn get_block_headers_after(
        &mut self,
        from_block: L2BlockNumber,
    ) -> DalResult<Vec<BlockHeader>> {
        let blocks_rows: Vec<_> = sqlx::query!(
            r#"
            SELECT
                miniblocks.hash AS "block_hash",
                miniblocks.number AS "block_number",
                prev_miniblock.hash AS "parent_hash?",
                miniblocks.timestamp AS "block_timestamp",
                miniblocks.base_fee_per_gas AS "base_fee_per_gas",
                miniblocks.gas_limit AS "block_gas_limit?",
                miniblocks.logs_bloom AS "block_logs_bloom?",
                transactions.gas_limit AS "transaction_gas_limit?",
                transactions.refunded_gas AS "transaction_refunded_gas?"
            FROM
                miniblocks
            LEFT JOIN
                miniblocks prev_miniblock
                ON prev_miniblock.number = miniblocks.number - 1
            LEFT JOIN transactions ON transactions.miniblock_number = miniblocks.number
            WHERE
                miniblocks.number > $1
            ORDER BY
                miniblocks.number ASC,
                transactions.index_in_block ASC
            "#,
            i64::from(from_block.0),
        )
        .instrument("get_block_headers_after")
        .with_arg("from_block", &from_block)
        .fetch_all(self.storage)
        .await?;

        let mut headers_map = std::collections::HashMap::new();

        for row in blocks_rows.iter() {
            let entry = headers_map
                .entry(row.block_number)
                .or_insert_with(|| BlockHeader {
                    hash: Some(H256::from_slice(&row.block_hash)),
                    parent_hash: row
                        .parent_hash
                        .as_deref()
                        .map_or_else(H256::zero, H256::from_slice),
                    uncles_hash: EMPTY_UNCLES_HASH,
                    author: H160::zero(),
                    state_root: H256::zero(),
                    transactions_root: H256::zero(),
                    receipts_root: H256::zero(),
                    number: Some(U64::from(row.block_number)),
                    gas_used: U256::zero(),
                    gas_limit: (row
                        .block_gas_limit
                        .unwrap_or(i64::from(LEGACY_BLOCK_GAS_LIMIT))
                        as u64)
                        .into(),
                    base_fee_per_gas: Some(bigdecimal_to_u256(row.base_fee_per_gas.clone())),
                    extra_data: Bytes::default(),
                    logs_bloom: row
                        .block_logs_bloom
                        .as_ref()
                        .map(|b| Bloom::from_slice(b))
                        .unwrap_or_default(),
                    timestamp: U256::from(row.block_timestamp),
                    difficulty: U256::zero(),
                    mix_hash: None,
                    nonce: None,
                });

            if let (Some(gas_limit), Some(refunded_gas)) = (
                row.transaction_gas_limit.clone(),
                row.transaction_refunded_gas,
            ) {
                entry.gas_used += bigdecimal_to_u256(gas_limit) - U256::from(refunded_gas as u64);
            }
        }

        let mut headers: Vec<BlockHeader> = headers_map.into_values().collect();
        headers.sort_by_key(|header| header.number);

        Ok(headers)
    }

    pub async fn resolve_block_id(
        &mut self,
        block_id: api::BlockId,
    ) -> DalResult<Option<L2BlockNumber>> {
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
                api::BlockId::Number(api::BlockNumber::L1Committed) => (
                    "
                    SELECT COALESCE(
                        (
                            SELECT MAX(number) FROM miniblocks
                            WHERE l1_batch_number = (
                                SELECT number FROM l1_batches
                                JOIN eth_txs_history ON
                                    l1_batches.eth_commit_tx_id = eth_txs_history.eth_tx_id
                                WHERE
                                    finality_status = 'finalized'
                                ORDER BY number DESC LIMIT 1
                            )
                        ),
                        0
                    ) AS number
                    ";
                ),
                api::BlockId::Number(api::BlockNumber::Precommitted) => (
                    // This query is used to get the latest precommitted miniblock number.
                    // If feature is not enabled, return the latest committed miniblock number.
                    // GREATEST in postgress ignore nulls.
                    "
                    SELECT GREATEST(
                        (
                            SELECT MAX(number)
                            FROM miniblocks
                            JOIN eth_txs_history ON
                                miniblocks.eth_precommit_tx_id = eth_txs_history.eth_tx_id
                            WHERE
                                eth_txs_history.finality_status = 'finalized'
                        ),
                        (
                            SELECT MAX(number)
                            FROM miniblocks
                            WHERE l1_batch_number =
                            (
                                SELECT number
                                FROM l1_batches
                                   JOIN eth_txs_history ON l1_batches.eth_commit_tx_id = eth_txs_history.eth_tx_id
                                WHERE eth_txs_history.finality_status = 'finalized'
                                ORDER BY number DESC
                                LIMIT 1
                            )
                        ),
                        0
                    ) AS number";
                ),
                api::BlockId::Number(api::BlockNumber::FastFinalized) => (
                    "
                    SELECT COALESCE(
                        (
                            SELECT MAX(number) FROM miniblocks
                            WHERE l1_batch_number = (
                                SELECT number FROM l1_batches
                                JOIN eth_txs_history ON
                                    l1_batches.eth_execute_tx_id = eth_txs_history.eth_tx_id
                                WHERE
                                    eth_txs_history.finality_status = 'fast_finalized'
                                    OR
                                    eth_txs_history.finality_status = 'finalized'
                                ORDER BY number DESC LIMIT 1
                            )
                        ),
                        0
                    ) AS number
                    ";
                ),

                api::BlockId::Number(api::BlockNumber::Finalized) => (
                    "
                    SELECT COALESCE(
                        (
                            SELECT MAX(number) FROM miniblocks
                            WHERE l1_batch_number = (
                                SELECT number FROM l1_batches
                                JOIN eth_txs_history ON
                                    l1_batches.eth_execute_tx_id = eth_txs_history.eth_tx_id
                                WHERE
                                    eth_txs_history.finality_status = 'finalized'
                                ORDER BY number DESC LIMIT 1
                            )
                        ),
                        0
                    ) AS number
                    ";
                ),
            }
        );

        let row = query
            .instrument("resolve_block_id")
            .with_arg("block_id", &block_id)
            .fetch_optional(self.storage)
            .await?;
        let block_number = row
            .and_then(|row| row.number)
            .map(|number| L2BlockNumber(number as u32));
        Ok(block_number)
    }

    /// Returns L1 batch timestamp for either sealed or pending L1 batch.
    ///
    /// The correctness of the current implementation depends on the timestamp of an L1 batch always
    /// being equal to the timestamp of the first L2 block in the batch.
    pub async fn get_expected_l1_batch_timestamp(
        &mut self,
        l1_batch_number: &ResolvedL1BatchForL2Block,
    ) -> DalResult<Option<u64>> {
        if let Some(l1_batch) = l1_batch_number.block_l1_batch {
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
                i64::from(l1_batch.0)
            )
            .instrument("get_expected_l1_batch_timestamp#sealed_l2_block")
            .with_arg("l1_batch_number", &l1_batch_number)
            .fetch_optional(self.storage)
            .await?
            .map(|row| row.timestamp as u64))
        } else {
            // Got a pending L2 block. Searching the timestamp of the first pending L2 block using
            // `WHERE l1_batch_number IS NULL` is slow since it potentially locks the `miniblocks` table.
            // Instead, we determine its number using the previous L1 batch, taking into the account that
            // it may be stored in the `snapshot_recovery` table.
            let prev_l1_batch_number = if l1_batch_number.pending_l1_batch == L1BatchNumber(0) {
                return Ok(None); // We haven't created the genesis L2 block yet
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
            .instrument("get_expected_l1_batch_timestamp#pending_l2_block")
            .with_arg("l1_batch_number", &l1_batch_number)
            .fetch_optional(self.storage)
            .await?
            .map(|row| row.timestamp as u64))
        }
    }

    pub async fn get_l2_block_hash(
        &mut self,
        block_number: L2BlockNumber,
    ) -> DalResult<Option<H256>> {
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
        .instrument("get_l2_block_hash")
        .with_arg("block_number", &block_number)
        .fetch_optional(self.storage)
        .await?
        .map(|row| H256::from_slice(&row.hash));
        Ok(hash)
    }

    pub async fn get_l2_to_l1_logs(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Vec<L2ToL1Log>> {
        self.storage
            .blocks_dal()
            .get_l2_to_l1_logs_for_batch::<L2ToL1Log>(l1_batch_number)
            .await
    }

    pub async fn get_l1_batch_number_of_l2_block(
        &mut self,
        l2_block_number: L2BlockNumber,
    ) -> DalResult<Option<L1BatchNumber>> {
        let number: Option<i64> = sqlx::query!(
            r#"
            SELECT
                l1_batch_number
            FROM
                miniblocks
            WHERE
                number = $1
            "#,
            i64::from(l2_block_number.0)
        )
        .instrument("get_l1_batch_number_of_l2_block")
        .with_arg("l2_block_number", &l2_block_number)
        .fetch_optional(self.storage)
        .await?
        .and_then(|row| row.l1_batch_number);

        Ok(number.map(|number| L1BatchNumber(number as u32)))
    }

    pub async fn get_l2_block_range_of_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<(L2BlockNumber, L2BlockNumber)>> {
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
        .instrument("get_l2_block_range_of_l1_batch")
        .with_arg("l1_batch_number", &l1_batch_number)
        .fetch_one(self.storage)
        .await?;

        Ok(match (row.min, row.max) {
            (Some(min), Some(max)) => Some((L2BlockNumber(min as u32), L2BlockNumber(max as u32))),
            (None, None) => None,
            _ => unreachable!(),
        })
    }

    pub async fn get_l1_batch_info_for_tx(
        &mut self,
        tx_hash: H256,
    ) -> DalResult<Option<(L1BatchNumber, u16)>> {
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
        .instrument("get_l1_batch_info_for_tx")
        .with_arg("tx_hash", &tx_hash)
        .fetch_optional(self.storage)
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

    /// Returns call traces for all transactions in the specified L2 block in the order of their execution.
    pub async fn get_traces_for_l2_block(
        &mut self,
        block_number: L2BlockNumber,
    ) -> DalResult<Vec<(Call, CallTraceMeta)>> {
        let row = sqlx::query!(
            r#"
            SELECT
                protocol_version,
                hash
            FROM
                miniblocks
            WHERE
                number = $1
            "#,
            i64::from(block_number.0)
        )
        .try_map(|row| {
            row.protocol_version
                .map(parse_protocol_version)
                .transpose()
                .map(|val| (val, H256::from_slice(&row.hash)))
        })
        .instrument("get_traces_for_l2_block#get_l2_block_protocol_version_id")
        .with_arg("l2_block_number", &block_number)
        .fetch_optional(self.storage)
        .await?;
        let Some((protocol_version, block_hash)) = row else {
            return Ok(Vec::new());
        };

        let protocol_version =
            protocol_version.unwrap_or_else(ProtocolVersionId::last_potentially_undefined);

        Ok(sqlx::query_as!(
            CallTrace,
            r#"
            SELECT
                transactions.hash AS tx_hash,
                transactions.index_in_block AS tx_index_in_block,
                call_trace,
                transactions.error AS tx_error
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
        .instrument("get_traces_for_l2_block")
        .with_arg("block_number", &block_number)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|mut call_trace| {
            let tx_hash = H256::from_slice(&call_trace.tx_hash);
            let index = call_trace.tx_index_in_block.unwrap_or_default() as usize;
            let meta = CallTraceMeta {
                index_in_block: index,
                tx_hash,
                block_number: block_number.0,
                block_hash,
                internal_error: call_trace.tx_error.take(),
            };
            (call_trace.into_call(protocol_version), meta)
        })
        .collect())
    }

    /// Returns `base_fee_per_gas` and `fair_pubdata_price` for L2 block range [min(newest_block - block_count + 1, 0), newest_block]
    /// in descending order of L2 block numbers.
    pub async fn get_fee_history(
        &mut self,
        newest_block: L2BlockNumber,
        block_count: u64,
    ) -> DalResult<(Vec<U256>, Vec<U256>)> {
        let result: Vec<_> = sqlx::query!(
            r#"
            SELECT
                base_fee_per_gas,
                l2_fair_gas_price,
                fair_pubdata_price,
                protocol_version,
                l1_gas_price
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
        .instrument("get_fee_history")
        .with_arg("newest_block", &newest_block)
        .with_arg("block_count", &block_count)
        .fetch_all(self.storage)
        .await?
        .into_iter()
        .map(|row| {
            let fee_input = BatchFeeInput::for_protocol_version(
                row.protocol_version
                    .map(|x| (x as u16).try_into().unwrap())
                    .unwrap_or_else(ProtocolVersionId::last_potentially_undefined),
                row.l2_fair_gas_price as u64,
                row.fair_pubdata_price.map(|x| x as u64),
                row.l1_gas_price as u64,
            );

            (
                bigdecimal_to_u256(row.base_fee_per_gas),
                U256::from(fee_input.fair_pubdata_price()),
            )
        })
        .collect();

        let (base_fee_per_gas, effective_pubdata_price): (Vec<U256>, Vec<U256>) =
            result.into_iter().unzip();

        Ok((base_fee_per_gas, effective_pubdata_price))
    }

    pub async fn get_block_details(
        &mut self,
        block_number: L2BlockNumber,
    ) -> DalResult<Option<api::BlockDetails>> {
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
                        WHERE
                            is_sealed
                    )
                ) AS "l1_batch_number!",
                miniblocks.timestamp,
                miniblocks.l1_tx_count,
                miniblocks.l2_tx_count,
                miniblocks.hash AS "root_hash?",
                commit_tx.tx_hash AS "commit_tx_hash?",
                commit_tx.confirmed_at AS "committed_at?",
                commit_tx.finality_status AS "commit_tx_finality_status?",
                commit_tx_data.chain_id AS "commit_chain_id?",
                prove_tx.tx_hash AS "prove_tx_hash?",
                prove_tx.confirmed_at AS "proven_at?",
                prove_tx.finality_status AS "prove_tx_finality_status?",
                prove_tx_data.chain_id AS "prove_chain_id?",
                execute_tx.tx_hash AS "execute_tx_hash?",
                execute_tx.finality_status AS "execute_tx_finality_status?",
                execute_tx.confirmed_at AS "executed_at?",
                execute_tx_data.chain_id AS "execute_chain_id?",
                precommit_tx.tx_hash AS "precommit_tx_hash?",
                precommit_tx.confirmed_at AS "precommitted_at?",
                precommit_tx.finality_status AS "precommit_tx_finality_status?",
                precommit_tx_data.chain_id AS "precommit_chain_id?",
                miniblocks.l1_gas_price,
                miniblocks.l2_fair_gas_price,
                miniblocks.fair_pubdata_price,
                miniblocks.bootloader_code_hash,
                miniblocks.default_aa_code_hash,
                l1_batches.evm_emulator_code_hash,
                miniblocks.protocol_version,
                miniblocks.fee_account_address
            FROM
                miniblocks
            LEFT JOIN l1_batches ON miniblocks.l1_batch_number = l1_batches.number
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
            LEFT JOIN eth_txs_history AS precommit_tx
                ON (
                    miniblocks.eth_precommit_tx_id = precommit_tx.eth_tx_id
                    AND precommit_tx.confirmed_at IS NOT NULL
                )
            LEFT JOIN eth_txs AS commit_tx_data
                ON (
                    l1_batches.eth_commit_tx_id = commit_tx_data.id
                    AND commit_tx_data.confirmed_eth_tx_history_id IS NOT NULL
                )
            LEFT JOIN eth_txs AS prove_tx_data
                ON (
                    l1_batches.eth_prove_tx_id = prove_tx_data.id
                    AND prove_tx_data.confirmed_eth_tx_history_id IS NOT NULL
                )
            LEFT JOIN eth_txs AS execute_tx_data
                ON (
                    l1_batches.eth_execute_tx_id = execute_tx_data.id
                    AND execute_tx_data.confirmed_eth_tx_history_id IS NOT NULL
                )
            LEFT JOIN eth_txs AS precommit_tx_data
                ON (
                    miniblocks.eth_precommit_tx_id = precommit_tx_data.id
                    AND precommit_tx_data.confirmed_eth_tx_history_id IS NOT NULL
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

        Ok(storage_block_details.map(Into::into))
    }

    pub async fn get_l1_batch_details(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<api::L1BatchDetails>> {
        let l1_batch_details: Option<StorageL1BatchDetails> = sqlx::query_as!(
            StorageL1BatchDetails,
            r#"
            WITH
            mb AS (
                SELECT
                    l1_gas_price,
                    l2_fair_gas_price,
                    fair_pubdata_price
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
                commit_tx.finality_status AS "commit_tx_finality_status?",
                commit_tx.confirmed_at AS "committed_at?",
                commit_tx_data.chain_id AS "commit_chain_id?",
                prove_tx.tx_hash AS "prove_tx_hash?",
                prove_tx.finality_status AS "prove_tx_finality_status?",
                prove_tx.confirmed_at AS "proven_at?",
                prove_tx_data.chain_id AS "prove_chain_id?",
                execute_tx.tx_hash AS "execute_tx_hash?",
                execute_tx.finality_status AS "execute_tx_finality_status?",
                execute_tx.confirmed_at AS "executed_at?",
                execute_tx_data.chain_id AS "execute_chain_id?",
                precommit_tx.tx_hash AS "precommit_tx_hash?",
                precommit_tx.confirmed_at AS "precommitted_at?",
                precommit_tx.finality_status AS "precommit_tx_finality_status?",
                precommit_tx_data.chain_id AS "precommit_chain_id?",
                mb.l1_gas_price,
                mb.l2_fair_gas_price,
                mb.fair_pubdata_price,
                l1_batches.bootloader_code_hash,
                l1_batches.default_aa_code_hash,
                l1_batches.evm_emulator_code_hash
            FROM
                l1_batches
            INNER JOIN mb ON TRUE
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
            LEFT JOIN eth_txs_history AS precommit_tx
                ON (
                    l1_batches.final_precommit_eth_tx_id = precommit_tx.eth_tx_id
                    AND precommit_tx.confirmed_at IS NOT NULL
                )
            
            LEFT JOIN eth_txs AS commit_tx_data
                ON (
                    l1_batches.eth_commit_tx_id = commit_tx_data.id
                    AND commit_tx_data.confirmed_eth_tx_history_id IS NOT NULL
                )
            LEFT JOIN eth_txs AS prove_tx_data
                ON (
                    l1_batches.eth_prove_tx_id = prove_tx_data.id
                    AND prove_tx_data.confirmed_eth_tx_history_id IS NOT NULL
                )
            LEFT JOIN eth_txs AS execute_tx_data
                ON (
                    l1_batches.eth_execute_tx_id = execute_tx_data.id
                    AND execute_tx_data.confirmed_eth_tx_history_id IS NOT NULL
                )
            LEFT JOIN eth_txs AS precommit_tx_data
                ON (
                    l1_batches.final_precommit_eth_tx_id = precommit_tx_data.id
                    AND precommit_tx_data.confirmed_eth_tx_history_id IS NOT NULL
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

    /// Returns L1 batch details returns batch transactions even if they are pending
    /// This is to be used in testing only as pending transactions are unverified
    pub async fn get_l1_batch_details_incl_unverified_transactions(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> DalResult<Option<api::L1BatchDetails>> {
        let l1_batch_details: Option<StorageL1BatchDetails> = sqlx::query_as!(
            StorageL1BatchDetails,
            r#"
            WITH
            mb AS (
                SELECT
                    l1_gas_price,
                    l2_fair_gas_price,
                    fair_pubdata_price
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
                commit_tx.finality_status AS "commit_tx_finality_status?",
                commit_tx.confirmed_at AS "committed_at?",
                commit_tx_data.chain_id AS "commit_chain_id?",
                prove_tx.tx_hash AS "prove_tx_hash?",
                prove_tx.finality_status AS "prove_tx_finality_status?",
                prove_tx.confirmed_at AS "proven_at?",
                prove_tx_data.chain_id AS "prove_chain_id?",
                execute_tx.tx_hash AS "execute_tx_hash?",
                execute_tx.finality_status AS "execute_tx_finality_status?",
                execute_tx.confirmed_at AS "executed_at?",
                execute_tx_data.chain_id AS "execute_chain_id?",
                precommit_tx.tx_hash AS "precommit_tx_hash?",
                precommit_tx.confirmed_at AS "precommitted_at?",
                precommit_tx.finality_status AS "precommit_tx_finality_status?",
                precommit_tx_data.chain_id AS "precommit_chain_id?",
                mb.l1_gas_price,
                mb.l2_fair_gas_price,
                mb.fair_pubdata_price,
                l1_batches.bootloader_code_hash,
                l1_batches.default_aa_code_hash,
                l1_batches.evm_emulator_code_hash
            FROM
                l1_batches
            INNER JOIN mb ON TRUE
            LEFT JOIN eth_txs_history AS commit_tx
                ON
                    l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id
            
            LEFT JOIN eth_txs_history AS prove_tx
                ON
                    l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id
            
            LEFT JOIN eth_txs_history AS execute_tx
                ON
                    l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id
            
            LEFT JOIN eth_txs_history AS precommit_tx
                ON
                    l1_batches.final_precommit_eth_tx_id = precommit_tx.eth_tx_id
            
            LEFT JOIN eth_txs AS commit_tx_data
                ON
                    l1_batches.eth_commit_tx_id = commit_tx_data.id
            
            LEFT JOIN eth_txs AS prove_tx_data
                ON
                    l1_batches.eth_prove_tx_id = prove_tx_data.id
            
            LEFT JOIN eth_txs AS execute_tx_data
                ON
                    l1_batches.eth_execute_tx_id = execute_tx_data.id
            
            LEFT JOIN eth_txs AS precommit_tx_data
                ON
                    l1_batches.final_precommit_eth_tx_id = precommit_tx_data.id
            
            WHERE
                l1_batches.number = $1
            "#,
            i64::from(l1_batch_number.0)
        )
        .instrument("get_l1_batch_details_with_pending_transactions")
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
        aggregated_operations::{AggregatedActionType, L1BatchAggregatedActionType},
        block::{L2BlockHasher, L2BlockHeader},
        eth_sender::EthTxFinalityStatus,
        Address, L2BlockNumber, ProtocolVersion, ProtocolVersionId,
    };
    use zksync_vm_interface::{tracer::ValidationTraces, TransactionExecutionMetrics};

    use super::*;
    use crate::{
        tests::{
            create_l1_batch_header, create_l2_block_header, create_snapshot_recovery,
            mock_execution_result, mock_l2_transaction,
        },
        ConnectionPool, Core, CoreDal,
    };

    #[tokio::test]
    async fn getting_web3_block_and_tx_count() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.blocks_dal()
            .delete_l2_blocks(L2BlockNumber(0))
            .await
            .unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        let header = L2BlockHeader {
            l1_tx_count: 3,
            l2_tx_count: 5,
            ..create_l2_block_header(0)
        };
        conn.blocks_dal().insert_l2_block(&header).await.unwrap();

        let block_hash = L2BlockHasher::new(L2BlockNumber(0), 0, H256::zero())
            .finalize(ProtocolVersionId::latest());
        let block = conn.blocks_web3_dal().get_api_block(L2BlockNumber(0)).await;
        let block = block.unwrap().unwrap();
        assert!(block.transactions.is_empty());
        assert_eq!(block.number, U64::zero());
        assert_eq!(block.hash, block_hash);

        let tx_count = conn
            .blocks_web3_dal()
            .get_block_tx_count(L2BlockNumber(0))
            .await;
        assert_eq!(tx_count.unwrap(), Some(8));

        let block = conn.blocks_web3_dal().get_api_block(L2BlockNumber(1)).await;
        assert!(block.unwrap().is_none());

        let tx_count = conn
            .blocks_web3_dal()
            .get_block_tx_count(L2BlockNumber(1))
            .await;
        assert_eq!(tx_count.unwrap(), None);
    }

    #[tokio::test]
    async fn resolving_earliest_block_id() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();

        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Earliest))
            .await;
        assert_eq!(l2_block_number.unwrap(), None);

        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(0))
            .await
            .unwrap();

        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Earliest))
            .await;
        assert_eq!(l2_block_number.unwrap(), Some(L2BlockNumber(0)));
    }

    #[tokio::test]
    async fn resolving_latest_block_id() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Latest))
            .await
            .unwrap();
        assert_eq!(l2_block_number, None);
        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Pending))
            .await
            .unwrap();
        assert_eq!(l2_block_number, Some(L2BlockNumber(0)));

        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(0))
            .await
            .unwrap();

        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Latest))
            .await;
        assert_eq!(l2_block_number.unwrap(), Some(L2BlockNumber(0)));

        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Number(0.into())))
            .await;
        assert_eq!(l2_block_number.unwrap(), Some(L2BlockNumber(0)));
        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Number(1.into())))
            .await;
        assert_eq!(l2_block_number.unwrap(), None);

        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(1))
            .await
            .unwrap();
        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Latest))
            .await;
        assert_eq!(l2_block_number.unwrap(), Some(L2BlockNumber(1)));

        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Pending))
            .await;
        assert_eq!(l2_block_number.unwrap(), Some(L2BlockNumber(2)));

        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Number(1.into())))
            .await;
        assert_eq!(l2_block_number.unwrap(), Some(L2BlockNumber(1)));
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

        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::Pending))
            .await
            .unwrap();
        assert_eq!(l2_block_number, Some(L2BlockNumber(43)));
    }

    #[tokio::test]
    async fn resolving_l1_committed_block_id() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();

        let l2_block_header = create_l2_block_header(1);
        conn.blocks_dal()
            .insert_l2_block(&l2_block_header)
            .await
            .unwrap();

        let l1_batch_header = create_l1_batch_header(0);

        conn.blocks_dal()
            .insert_mock_l1_batch(&l1_batch_header)
            .await
            .unwrap();
        conn.blocks_dal()
            .mark_l2_blocks_as_executed_in_l1_batch(l1_batch_header.number)
            .await
            .unwrap();

        let resolved_l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::L1Committed))
            .await
            .unwrap();
        assert_eq!(resolved_l2_block_number, Some(L2BlockNumber(0)));

        let mocked_commit_eth_tx = conn
            .eth_sender_dal()
            .save_eth_tx(
                0,
                vec![],
                AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Commit),
                Address::default(),
                None,
                None,
                None,
                false,
            )
            .await
            .unwrap();
        let tx_hash = H256::random();
        conn.eth_sender_dal()
            .insert_tx_history(
                mocked_commit_eth_tx.id,
                0,
                0,
                None,
                None,
                tx_hash,
                &[],
                0,
                None,
            )
            .await
            .unwrap();
        conn.eth_sender_dal()
            .confirm_tx(tx_hash, EthTxFinalityStatus::Finalized, U256::zero())
            .await
            .unwrap();
        conn.blocks_dal()
            .set_eth_tx_id_for_l1_batches(
                l1_batch_header.number..=l1_batch_header.number,
                mocked_commit_eth_tx.id,
                AggregatedActionType::L1Batch(L1BatchAggregatedActionType::Commit),
            )
            .await
            .unwrap();

        let resolved_l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Number(api::BlockNumber::L1Committed))
            .await
            .unwrap();

        assert_eq!(resolved_l2_block_number, Some(l2_block_header.number));
    }

    #[tokio::test]
    async fn resolving_block_by_hash() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(0))
            .await
            .unwrap();

        let hash = L2BlockHasher::new(L2BlockNumber(0), 0, H256::zero())
            .finalize(ProtocolVersionId::latest());
        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Hash(hash))
            .await;
        assert_eq!(l2_block_number.unwrap(), Some(L2BlockNumber(0)));

        let hash = L2BlockHasher::new(L2BlockNumber(1), 1, H256::zero())
            .finalize(ProtocolVersionId::latest());
        let l2_block_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Hash(hash))
            .await;
        assert_eq!(l2_block_number.unwrap(), None);
    }

    #[tokio::test]
    async fn getting_traces_for_block() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = connection_pool.connection().await.unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(&ProtocolVersion::default())
            .await
            .unwrap();
        conn.blocks_dal()
            .insert_l2_block(&create_l2_block_header(1))
            .await
            .unwrap();

        let transactions = [mock_l2_transaction(), mock_l2_transaction()];
        let mut tx_results = vec![];
        for (i, tx) in transactions.into_iter().enumerate() {
            conn.transactions_dal()
                .insert_transaction_l2(
                    &tx,
                    TransactionExecutionMetrics::default(),
                    ValidationTraces::default(),
                )
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
            .mark_txs_as_executed_in_l2_block(
                L2BlockNumber(1),
                &tx_results,
                1.into(),
                ProtocolVersionId::latest(),
                false,
            )
            .await
            .unwrap();

        let traces = conn
            .blocks_web3_dal()
            .get_traces_for_l2_block(L2BlockNumber(1))
            .await
            .unwrap();
        assert_eq!(traces.len(), 2);
        for ((trace, meta), tx_result) in traces.iter().zip(&tx_results) {
            let expected_trace = tx_result.call_trace().unwrap();
            assert_eq!(tx_result.hash, meta.tx_hash);
            assert_eq!(*trace, expected_trace);
        }
    }
}
