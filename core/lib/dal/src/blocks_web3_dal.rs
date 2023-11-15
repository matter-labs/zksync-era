use bigdecimal::BigDecimal;
use sqlx::Row;

use zksync_system_constants::EMPTY_UNCLES_HASH;
use zksync_types::{
    api,
    ethabi::Address,
    l2_to_l1_log::L2ToL1Log,
    vm_trace::Call,
    web3::types::{BlockHeader, U64},
    zkevm_test_harness::zk_evm::zkevm_opcode_defs::system_params,
    Bytes, L1BatchNumber, L2ChainId, MiniblockNumber, H160, H2048, H256, U256,
};
use zksync_utils::bigdecimal_to_u256;

use crate::models::{
    storage_block::{
        bind_block_where_sql_params, web3_block_number_to_sql, web3_block_where_sql,
        StorageBlockDetails, StorageL1BatchDetails,
    },
    storage_transaction::{extract_web3_transaction, web3_transaction_select_sql, CallTrace},
};
use crate::{instrument::InstrumentExt, StorageProcessor};

const BLOCK_GAS_LIMIT: u32 = system_params::VM_INITIAL_FRAME_ERGS;

#[derive(Debug)]
pub struct BlocksWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl BlocksWeb3Dal<'_, '_> {
    pub async fn get_sealed_miniblock_number(&mut self) -> sqlx::Result<MiniblockNumber> {
        let number = sqlx::query!("SELECT MAX(number) as \"number\" FROM miniblocks")
            .instrument("get_sealed_block_number")
            .report_latency()
            .fetch_one(self.storage.conn())
            .await?
            .number
            .expect("DAL invocation before genesis");
        Ok(MiniblockNumber(number as u32))
    }

    pub async fn get_sealed_l1_batch_number(&mut self) -> sqlx::Result<L1BatchNumber> {
        let number = sqlx::query!("SELECT MAX(number) as \"number\" FROM l1_batches")
            .instrument("get_sealed_block_number")
            .report_latency()
            .fetch_one(self.storage.conn())
            .await?
            .number
            .expect("DAL invocation before genesis");
        Ok(L1BatchNumber(number as u32))
    }

    pub async fn get_block_by_web3_block_id(
        &mut self,
        block_id: api::BlockId,
        include_full_transactions: bool,
        chain_id: L2ChainId,
    ) -> sqlx::Result<Option<api::Block<api::TransactionVariant>>> {
        let transactions_sql = if include_full_transactions {
            web3_transaction_select_sql()
        } else {
            "transactions.hash as tx_hash"
        };

        let query = format!(
            "SELECT
                miniblocks.hash as block_hash,
                miniblocks.number,
                miniblocks.l1_batch_number,
                miniblocks.timestamp,
                miniblocks.base_fee_per_gas,
                prev_miniblock.hash as parent_hash,
                l1_batches.timestamp as l1_batch_timestamp,
                transactions.gas_limit as gas_limit,
                transactions.refunded_gas as refunded_gas,
                {}
            FROM miniblocks
            LEFT JOIN miniblocks prev_miniblock
                ON prev_miniblock.number = miniblocks.number - 1
            LEFT JOIN l1_batches
                ON l1_batches.number = miniblocks.l1_batch_number
            LEFT JOIN transactions
                ON transactions.miniblock_number = miniblocks.number
            WHERE {}",
            transactions_sql,
            web3_block_where_sql(block_id, 1)
        );

        let query = bind_block_where_sql_params(&block_id, sqlx::query(&query));
        let rows = query.fetch_all(self.storage.conn()).await?.into_iter();

        let block = rows.fold(None, |prev_block, db_row| {
            let mut block = prev_block.unwrap_or_else(|| {
                // This code will be only executed for the first row in the DB response.
                // All other rows will only be used to extract relevant transactions.
                let hash = db_row
                    .try_get("block_hash")
                    .map_or_else(|_| H256::zero(), H256::from_slice);
                let number = U64::from(db_row.get::<i64, &str>("number"));
                let l1_batch_number = db_row
                    .try_get::<i64, &str>("l1_batch_number")
                    .map(U64::from)
                    .ok();
                let l1_batch_timestamp = db_row
                    .try_get::<i64, &str>("l1_batch_timestamp")
                    .map(U256::from)
                    .ok();
                let parent_hash = db_row
                    .try_get("parent_hash")
                    .map_or_else(|_| H256::zero(), H256::from_slice);
                let base_fee_per_gas = db_row.get::<BigDecimal, &str>("base_fee_per_gas");

                api::Block {
                    hash,
                    parent_hash,
                    uncles_hash: EMPTY_UNCLES_HASH,
                    number,
                    l1_batch_number,
                    gas_limit: BLOCK_GAS_LIMIT.into(),
                    base_fee_per_gas: bigdecimal_to_u256(base_fee_per_gas),
                    timestamp: db_row.get::<i64, &str>("timestamp").into(),
                    l1_batch_timestamp,
                    // TODO: include logs
                    ..api::Block::default()
                }
            });
            if db_row.try_get::<&[u8], &str>("tx_hash").is_ok() {
                let tx_gas_limit = bigdecimal_to_u256(db_row.get::<BigDecimal, &str>("gas_limit"));
                let tx_refunded_gas = U256::from((db_row.get::<i64, &str>("refunded_gas")) as u32);

                block.gas_used += tx_gas_limit - tx_refunded_gas;
                let tx = if include_full_transactions {
                    let tx = extract_web3_transaction(db_row, chain_id);
                    api::TransactionVariant::Full(tx)
                } else {
                    api::TransactionVariant::Hash(H256::from_slice(db_row.get("tx_hash")))
                };
                block.transactions.push(tx);
            }
            Some(block)
        });
        Ok(block)
    }

    pub async fn get_block_tx_count(
        &mut self,
        block_id: api::BlockId,
    ) -> sqlx::Result<Option<(MiniblockNumber, U256)>> {
        let query = format!(
            "SELECT number, l1_tx_count + l2_tx_count AS tx_count FROM miniblocks WHERE {}",
            web3_block_where_sql(block_id, 1)
        );
        let query = bind_block_where_sql_params(&block_id, sqlx::query(&query));

        Ok(query.fetch_optional(self.storage.conn()).await?.map(|row| {
            let miniblock_number = row.get::<i64, _>("number") as u32;
            let tx_count = row.get::<i32, _>("tx_count") as u32;
            (MiniblockNumber(miniblock_number), tx_count.into())
        }))
    }

    /// Returns hashes of blocks with numbers greater than `from_block` and the number of the last block.
    pub async fn get_block_hashes_after(
        &mut self,
        from_block: MiniblockNumber,
        limit: usize,
    ) -> sqlx::Result<(Vec<H256>, Option<MiniblockNumber>)> {
        let rows = sqlx::query!(
            "SELECT number, hash FROM miniblocks \
            WHERE number > $1 \
            ORDER BY number ASC \
            LIMIT $2",
            from_block.0 as i64,
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
            "SELECT hash, number, timestamp \
            FROM miniblocks \
            WHERE number > $1 \
            ORDER BY number ASC",
            from_block.0 as i64,
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
        let query_string = match block_id {
            api::BlockId::Hash(_) => "SELECT number FROM miniblocks WHERE hash = $1".to_owned(),
            api::BlockId::Number(api::BlockNumber::Number(_)) => {
                // The reason why instead of returning the `block_number` directly we use query is
                // to handle numbers of blocks that are not created yet.
                // the `SELECT number FROM miniblocks WHERE number=block_number` for
                // non-existing block number will returns zero.
                "SELECT number FROM miniblocks WHERE number = $1".to_owned()
            }
            api::BlockId::Number(api::BlockNumber::Earliest) => {
                return Ok(Some(MiniblockNumber(0)));
            }
            api::BlockId::Number(block_number) => web3_block_number_to_sql(block_number),
        };
        let row = bind_block_where_sql_params(&block_id, sqlx::query(&query_string))
            .fetch_optional(self.storage.conn())
            .await?;

        let block_number = row
            .and_then(|row| row.get::<Option<i64>, &str>("number"))
            .map(|n| MiniblockNumber(n as u32));
        Ok(block_number)
    }

    /// Returns L1 batch timestamp for either sealed or pending L1 batch.
    pub async fn get_expected_l1_batch_timestamp(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<u64>> {
        let first_miniblock_of_batch = if l1_batch_number.0 == 0 {
            MiniblockNumber(0)
        } else {
            match self
                .get_miniblock_range_of_l1_batch(l1_batch_number - 1)
                .await?
            {
                Some((_, miniblock_number)) => miniblock_number + 1,
                None => return Ok(None),
            }
        };
        let timestamp = sqlx::query!(
            "SELECT timestamp FROM miniblocks \
            WHERE number = $1",
            first_miniblock_of_batch.0 as i64
        )
        .fetch_optional(self.storage.conn())
        .await?
        .map(|row| row.timestamp as u64);
        Ok(timestamp)
    }

    pub async fn get_miniblock_hash(
        &mut self,
        block_number: MiniblockNumber,
    ) -> sqlx::Result<Option<H256>> {
        let hash = sqlx::query!(
            "SELECT hash FROM miniblocks WHERE number = $1",
            block_number.0 as i64
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
            "SELECT l2_to_l1_logs FROM l1_batches WHERE number = $1",
            block_number.0 as i64
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
            "SELECT l1_batch_number FROM miniblocks WHERE number = $1",
            miniblock_number.0 as i64
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
            "SELECT MIN(miniblocks.number) as \"min?\", MAX(miniblocks.number) as \"max?\" \
            FROM miniblocks \
            WHERE l1_batch_number = $1",
            l1_batch_number.0 as i64
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
            "SELECT l1_batch_number, l1_batch_tx_index \
            FROM transactions \
            WHERE hash = $1",
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

    pub async fn get_trace_for_miniblock(
        &mut self,
        block_number: MiniblockNumber,
    ) -> sqlx::Result<Vec<Call>> {
        Ok(sqlx::query_as!(
            CallTrace,
            "SELECT * FROM call_traces WHERE tx_hash IN \
                (SELECT hash FROM transactions WHERE miniblock_number = $1)",
            block_number.0 as i64
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
            "SELECT base_fee_per_gas FROM miniblocks \
            WHERE number <= $1 \
            ORDER BY number DESC LIMIT $2",
            newest_block.0 as i64,
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
        current_operator_address: Address,
    ) -> sqlx::Result<Option<api::BlockDetails>> {
        {
            let storage_block_details = sqlx::query_as!(
                StorageBlockDetails,
                r#"
                    SELECT miniblocks.number,
                        COALESCE(miniblocks.l1_batch_number, (SELECT (max(number) + 1) FROM l1_batches)) as "l1_batch_number!",
                        miniblocks.timestamp,
                        miniblocks.l1_tx_count,
                        miniblocks.l2_tx_count,
                        miniblocks.hash as "root_hash?",
                        commit_tx.tx_hash as "commit_tx_hash?",
                        commit_tx.confirmed_at as "committed_at?",
                        prove_tx.tx_hash as "prove_tx_hash?",
                        prove_tx.confirmed_at as "proven_at?",
                        execute_tx.tx_hash as "execute_tx_hash?",
                        execute_tx.confirmed_at as "executed_at?",
                        miniblocks.l1_gas_price,
                        miniblocks.l2_fair_gas_price,
                        miniblocks.bootloader_code_hash,
                        miniblocks.default_aa_code_hash,
                        miniblocks.protocol_version,
                        l1_batches.fee_account_address as "fee_account_address?"
                    FROM miniblocks
                    LEFT JOIN l1_batches ON miniblocks.l1_batch_number = l1_batches.number
                    LEFT JOIN eth_txs_history as commit_tx ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id AND commit_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs_history as prove_tx ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id AND prove_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs_history as execute_tx ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id AND execute_tx.confirmed_at IS NOT NULL)
                    WHERE miniblocks.number = $1
                "#,
                block_number.0 as i64
            )
            .instrument("get_block_details")
            .with_arg("block_number", &block_number)
            .report_latency()
            .fetch_optional(self.storage.conn())
            .await?;

            Ok(storage_block_details.map(|storage_block_details| {
                storage_block_details.into_block_details(current_operator_address)
            }))
        }
    }

    pub async fn get_l1_batch_details(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> sqlx::Result<Option<api::L1BatchDetails>> {
        {
            let l1_batch_details: Option<StorageL1BatchDetails> = sqlx::query_as!(
                StorageL1BatchDetails,
                r#"
                    SELECT l1_batches.number,
                        l1_batches.timestamp,
                        l1_batches.l1_tx_count,
                        l1_batches.l2_tx_count,
                        l1_batches.hash as "root_hash?",
                        commit_tx.tx_hash as "commit_tx_hash?",
                        commit_tx.confirmed_at as "committed_at?",
                        prove_tx.tx_hash as "prove_tx_hash?",
                        prove_tx.confirmed_at as "proven_at?",
                        execute_tx.tx_hash as "execute_tx_hash?",
                        execute_tx.confirmed_at as "executed_at?",
                        l1_batches.l1_gas_price,
                        l1_batches.l2_fair_gas_price,
                        l1_batches.bootloader_code_hash,
                        l1_batches.default_aa_code_hash
                    FROM l1_batches
                    LEFT JOIN eth_txs_history as commit_tx ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id AND commit_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs_history as prove_tx ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id AND prove_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs_history as execute_tx ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id AND execute_tx.confirmed_at IS NOT NULL)
                    WHERE l1_batches.number = $1
                "#,
                l1_batch_number.0 as i64
            )
            .instrument("get_l1_batch_details")
            .with_arg("l1_batch_number", &l1_batch_number)
            .report_latency()
            .fetch_optional(self.storage.conn())
            .await?;

            Ok(l1_batch_details.map(api::L1BatchDetails::from))
        }
    }

    pub async fn get_miniblock_for_virtual_block_from(
        &mut self,
        migration_start_l1_batch_number: u64,
        from_virtual_block_number: u64,
    ) -> sqlx::Result<Option<u32>> {
        // Since virtual blocks are numerated from `migration_start_l1_batch_number` number and not from 0
        // we have to subtract (migration_start_l1_batch_number - 1) from the `from` virtual block
        // to find miniblock using query below
        let virtual_block_offset = from_virtual_block_number - migration_start_l1_batch_number + 1;

        // In the query below `virtual_block_sum` is actually latest virtual block number, created within this miniblock
        // and that can be calculated as sum of all virtual blocks counts, created in previous miniblocks.
        // It is considered that all logs are created in the last virtual block of this miniblock,
        // that's why we are interested in funding it.
        // The goal of this query is to find the first miniblock, which contains given virtual block.
        let record = sqlx::query!(
            "SELECT number \
            FROM ( \
                SELECT number, sum(virtual_blocks) OVER(ORDER BY number) AS virtual_block_sum \
                FROM miniblocks \
                WHERE l1_batch_number >= $1 \
            ) AS vts \
            WHERE virtual_block_sum >= $2 \
            ORDER BY number LIMIT 1",
            migration_start_l1_batch_number as i64,
            virtual_block_offset as i64
        )
        .instrument("get_miniblock_for_virtual_block_from")
        .with_arg(
            "migration_start_l1_batch_number",
            &migration_start_l1_batch_number,
        )
        .report_latency()
        .fetch_optional(self.storage.conn())
        .await?;

        let result = record.map(|row| row.number as u32);

        Ok(result)
    }

    pub async fn get_miniblock_for_virtual_block_to(
        &mut self,
        migration_start_l1_batch_number: u64,
        to_virtual_block_number: u64,
    ) -> sqlx::Result<Option<u32>> {
        // Since virtual blocks are numerated from `migration_start_l1_batch_number` number and not from 0
        // we have to subtract (migration_start_l1_batch_number - 1) from the `to` virtual block
        // to find miniblock using query below
        let virtual_block_offset = to_virtual_block_number - migration_start_l1_batch_number + 1;

        // In the query below `virtual_block_sum` is actually latest virtual block number, created within this miniblock
        // and that can be calculated as sum of all virtual blocks counts, created in previous miniblocks.
        // It is considered that all logs are created in the last virtual block of this miniblock,
        // that's why we are interested in funding it.
        // The goal of this query is to find the last miniblock, that contains logs all logs(in the last virtual block),
        // created before or in a given virtual block.
        let record = sqlx::query!(
            "SELECT number \
            FROM ( \
                SELECT number, sum(virtual_blocks) OVER(ORDER BY number) AS virtual_block_sum \
                FROM miniblocks \
                WHERE l1_batch_number >= $1 \
            ) AS vts \
            WHERE virtual_block_sum <= $2 \
            ORDER BY number DESC LIMIT 1",
            migration_start_l1_batch_number as i64,
            virtual_block_offset as i64
        )
        .instrument("get_miniblock_for_virtual_block_to")
        .with_arg(
            "migration_start_l1_batch_number",
            &migration_start_l1_batch_number,
        )
        .report_latency()
        .fetch_optional(self.storage.conn())
        .await?;

        let result = record.map(|row| row.number as u32);

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use zksync_contracts::BaseSystemContractsHashes;
    use zksync_types::{
        block::{miniblock_hash, MiniblockHeader},
        MiniblockNumber, ProtocolVersion, ProtocolVersionId,
    };

    use super::*;
    use crate::{tests::create_miniblock_header, ConnectionPool};

    #[tokio::test]
    async fn getting_web3_block_and_tx_count() {
        let connection_pool = ConnectionPool::test_pool().await;
        let mut conn = connection_pool.access_storage().await.unwrap();
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

        let block_ids = [
            api::BlockId::Number(api::BlockNumber::Earliest),
            api::BlockId::Number(api::BlockNumber::Latest),
            api::BlockId::Number(api::BlockNumber::Number(0.into())),
            api::BlockId::Hash(miniblock_hash(
                MiniblockNumber(0),
                0,
                H256::zero(),
                H256::zero(),
            )),
        ];
        for block_id in block_ids {
            let block = conn
                .blocks_web3_dal()
                .get_block_by_web3_block_id(block_id, false, L2ChainId::from(270))
                .await;
            let block = block.unwrap().unwrap();
            assert!(block.transactions.is_empty());
            assert_eq!(block.number, U64::zero());
            assert_eq!(
                block.hash,
                miniblock_hash(MiniblockNumber(0), 0, H256::zero(), H256::zero())
            );

            let tx_count = conn.blocks_web3_dal().get_block_tx_count(block_id).await;
            assert_eq!(tx_count.unwrap(), Some((MiniblockNumber(0), 8.into())));
        }

        let non_existing_block_ids = [
            api::BlockId::Number(api::BlockNumber::Pending),
            api::BlockId::Number(api::BlockNumber::Number(1.into())),
            api::BlockId::Hash(miniblock_hash(
                MiniblockNumber(1),
                1,
                H256::zero(),
                H256::zero(),
            )),
        ];
        for block_id in non_existing_block_ids {
            let block = conn
                .blocks_web3_dal()
                .get_block_by_web3_block_id(block_id, false, L2ChainId::from(270))
                .await;
            assert!(block.unwrap().is_none());

            let tx_count = conn.blocks_web3_dal().get_block_tx_count(block_id).await;
            assert_eq!(tx_count.unwrap(), None);
        }
    }

    #[tokio::test]
    async fn resolving_earliest_block_id() {
        let connection_pool = ConnectionPool::test_pool().await;
        let mut conn = connection_pool.access_storage().await.unwrap();
        conn.blocks_dal()
            .delete_miniblocks(MiniblockNumber(0))
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
        let connection_pool = ConnectionPool::test_pool().await;
        let mut conn = connection_pool.access_storage().await.unwrap();
        conn.blocks_dal()
            .delete_miniblocks(MiniblockNumber(0))
            .await
            .unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
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
    async fn resolving_block_by_hash() {
        let connection_pool = ConnectionPool::test_pool().await;
        let mut conn = connection_pool.access_storage().await.unwrap();
        conn.blocks_dal()
            .delete_miniblocks(MiniblockNumber(0))
            .await
            .unwrap();
        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;
        conn.blocks_dal()
            .insert_miniblock(&create_miniblock_header(0))
            .await
            .unwrap();

        let hash = miniblock_hash(MiniblockNumber(0), 0, H256::zero(), H256::zero());
        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Hash(hash))
            .await;
        assert_eq!(miniblock_number.unwrap(), Some(MiniblockNumber(0)));

        let hash = miniblock_hash(MiniblockNumber(1), 1, H256::zero(), H256::zero());
        let miniblock_number = conn
            .blocks_web3_dal()
            .resolve_block_id(api::BlockId::Hash(hash))
            .await;
        assert_eq!(miniblock_number.unwrap(), None);
    }

    #[tokio::test]
    async fn getting_miniblocks_for_virtual_block() {
        let connection_pool = ConnectionPool::test_pool().await;
        let mut conn = connection_pool.access_storage().await.unwrap();

        conn.protocol_versions_dal()
            .save_protocol_version_with_tx(ProtocolVersion::default())
            .await;

        let mut header = MiniblockHeader {
            number: MiniblockNumber(0),
            timestamp: 0,
            hash: miniblock_hash(MiniblockNumber(0), 0, H256::zero(), H256::zero()),
            l1_tx_count: 0,
            l2_tx_count: 0,
            base_fee_per_gas: 100,
            l1_gas_price: 100,
            l2_fair_gas_price: 100,
            base_system_contracts_hashes: BaseSystemContractsHashes::default(),
            protocol_version: Some(ProtocolVersionId::default()),
            virtual_blocks: 0,
        };
        conn.blocks_dal().insert_miniblock(&header).await.unwrap();
        conn.blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(0))
            .await
            .unwrap();

        header.number = MiniblockNumber(1);
        conn.blocks_dal().insert_miniblock(&header).await.unwrap();
        conn.blocks_dal()
            .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(1))
            .await
            .unwrap();

        for i in 2..=100 {
            header.number = MiniblockNumber(i);
            header.virtual_blocks = 5;

            conn.blocks_dal().insert_miniblock(&header).await.unwrap();
            conn.blocks_dal()
                .mark_miniblocks_as_executed_in_l1_batch(L1BatchNumber(i))
                .await
                .unwrap();
        }

        let virtual_block_ranges = [
            (2, 4),
            (20, 24),
            (11, 15),
            (1, 10),
            (88, 99),
            (1, 100),
            (1000000, 10000000),
        ];
        let expected_miniblock_ranges = [
            (Some(2), Some(1)),
            (Some(5), Some(5)),
            (Some(4), Some(4)),
            (Some(2), Some(3)),
            (Some(19), Some(20)),
            (Some(2), Some(21)),
            (None, Some(100)),
        ];

        let inputs_with_expected_values =
            IntoIterator::into_iter(virtual_block_ranges).zip(expected_miniblock_ranges);
        for (
            (virtual_block_start, virtual_block_end),
            (expected_miniblock_from, expected_miniblock_to),
        ) in inputs_with_expected_values
        {
            // migration_start_l1_batch_number = 1
            let miniblock_from = conn
                .blocks_web3_dal()
                .get_miniblock_for_virtual_block_from(1, virtual_block_start)
                .await
                .unwrap();
            assert_eq!(miniblock_from, expected_miniblock_from);

            let miniblock_to = conn
                .blocks_web3_dal()
                .get_miniblock_for_virtual_block_to(1, virtual_block_end)
                .await
                .unwrap();
            assert_eq!(miniblock_to, expected_miniblock_to);
        }
    }
}
