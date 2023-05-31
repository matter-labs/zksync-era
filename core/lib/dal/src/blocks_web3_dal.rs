use crate::models::storage_block::{
    bind_block_where_sql_params, web3_block_number_to_sql, web3_block_where_sql,
};
use crate::models::storage_transaction::{extract_web3_transaction, web3_transaction_select_sql};
use crate::SqlxError;
use crate::StorageProcessor;
use bigdecimal::BigDecimal;
use sqlx::postgres::PgArguments;
use sqlx::query::Query;
use sqlx::{Postgres, Row};
use std::time::Instant;
use vm::utils::BLOCK_GAS_LIMIT;
use zksync_config::constants::EMPTY_UNCLES_HASH;

use crate::models::storage_transaction::CallTrace;
use zksync_types::api::{self, Block, BlockId, TransactionVariant};
use zksync_types::l2_to_l1_log::L2ToL1Log;
use zksync_types::vm_trace::Call;
use zksync_types::web3::types::{BlockHeader, U64};
use zksync_types::{L1BatchNumber, L2ChainId, MiniblockNumber, H160, H256, U256};
use zksync_utils::{bigdecimal_to_u256, miniblock_hash};
use zksync_web3_decl::error::Web3Error;

#[derive(Debug)]
pub struct BlocksWeb3Dal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl BlocksWeb3Dal<'_, '_> {
    pub fn get_sealed_miniblock_number(&mut self) -> Result<MiniblockNumber, SqlxError> {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            let number: i64 = sqlx::query!(r#"SELECT MAX(number) as "number" FROM miniblocks"#)
                .fetch_one(self.storage.conn())
                .await?
                .number
                .expect("DAL invocation before genesis");
            metrics::histogram!("dal.request", started_at.elapsed(), "method" => "get_sealed_block_number");
            Ok(MiniblockNumber(number as u32))
        })
    }

    pub fn get_sealed_l1_batch_number(&mut self) -> Result<L1BatchNumber, SqlxError> {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            let number: i64 = sqlx::query!(r#"SELECT MAX(number) as "number" FROM l1_batches"#)
                .fetch_one(self.storage.conn())
                .await?
                .number
                .expect("DAL invocation before genesis");
            metrics::histogram!("dal.request", started_at.elapsed(), "method" => "get_sealed_block_number");
            Ok(L1BatchNumber(number as u32))
        })
    }

    pub fn get_block_by_web3_block_id(
        &mut self,
        block_id: BlockId,
        include_full_transactions: bool,
        chain_id: L2ChainId,
    ) -> Result<Option<Block<TransactionVariant>>, SqlxError> {
        async_std::task::block_on(async {
            let transactions_sql = if include_full_transactions {
                web3_transaction_select_sql()
            } else {
                "transactions.hash as tx_hash"
            };

            let query = format!(
                "
                SELECT
                    miniblocks.hash as block_hash,
                    miniblocks.number,
                    miniblocks.l1_batch_number,
                    miniblocks.timestamp,
                    miniblocks.base_fee_per_gas,
                    l1_batches.timestamp as l1_batch_timestamp,
                    transactions.gas_limit as gas_limit,
                    transactions.refunded_gas as refunded_gas,
                    {}
                FROM miniblocks
                LEFT JOIN l1_batches
                    ON l1_batches.number = miniblocks.l1_batch_number
                LEFT JOIN transactions
                    ON transactions.miniblock_number = miniblocks.number
                WHERE {}
                ",
                transactions_sql,
                web3_block_where_sql(block_id, 1)
            );

            let query: Query<Postgres, PgArguments> =
                bind_block_where_sql_params(block_id, sqlx::query(&query));

            let block = query
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .fold(
                    Option::<Block<TransactionVariant>>::None,
                    |prev_block, db_row| {
                        let mut block: Block<TransactionVariant> = prev_block.unwrap_or({
                            // This code will be only executed for the first row in the DB response.
                            // All other rows will only be used to extract relevant transactions.
                            let hash = db_row
                                .try_get("block_hash")
                                .map(H256::from_slice)
                                .unwrap_or_else(|_| H256::zero());
                            let number = U64::from(db_row.get::<i64, &str>("number"));
                            let l1_batch_number = db_row
                                .try_get::<i64, &str>("l1_batch_number")
                                .map(U64::from)
                                .ok();
                            let l1_batch_timestamp = db_row
                                .try_get::<i64, &str>("l1_batch_timestamp")
                                .map(U256::from)
                                .ok();
                            let parent_hash = match number.as_u32() {
                                0 => H256::zero(),
                                number => miniblock_hash(MiniblockNumber(number - 1)),
                            };

                            Block {
                                hash,
                                parent_hash,
                                uncles_hash: EMPTY_UNCLES_HASH,
                                author: H160::zero(),
                                state_root: H256::zero(),
                                transactions_root: H256::zero(),
                                receipts_root: H256::zero(),
                                number,
                                l1_batch_number,
                                gas_used: Default::default(),
                                gas_limit: BLOCK_GAS_LIMIT.into(),
                                base_fee_per_gas: bigdecimal_to_u256(
                                    db_row.get::<BigDecimal, &str>("base_fee_per_gas"),
                                ),
                                extra_data: Default::default(),
                                // todo logs
                                logs_bloom: Default::default(),
                                timestamp: U256::from(db_row.get::<i64, &str>("timestamp")),
                                l1_batch_timestamp,
                                difficulty: Default::default(),
                                total_difficulty: Default::default(),
                                seal_fields: vec![],
                                uncles: vec![],
                                transactions: Vec::default(),
                                size: Default::default(),
                                mix_hash: Default::default(),
                                nonce: Default::default(),
                            }
                        });
                        if db_row.try_get::<&[u8], &str>("tx_hash").is_ok() {
                            let tx_gas_limit: U256 =
                                bigdecimal_to_u256(db_row.get::<BigDecimal, &str>("gas_limit"));
                            let tx_refunded_gas: U256 =
                                ((db_row.get::<i64, &str>("refunded_gas")) as u32).into();

                            block.gas_used += tx_gas_limit - tx_refunded_gas;
                            let tx = if include_full_transactions {
                                TransactionVariant::Full(extract_web3_transaction(db_row, chain_id))
                            } else {
                                TransactionVariant::Hash(H256::from_slice(db_row.get("tx_hash")))
                            };
                            block.transactions.push(tx);
                        }
                        Some(block)
                    },
                );
            Ok(block)
        })
    }

    pub fn get_block_tx_count(&mut self, block_id: BlockId) -> Result<Option<U256>, SqlxError> {
        async_std::task::block_on(async {
            let query = format!(
                "SELECT l1_tx_count + l2_tx_count as tx_count FROM miniblocks WHERE {}",
                web3_block_where_sql(block_id, 1)
            );
            let query: Query<Postgres, PgArguments> =
                bind_block_where_sql_params(block_id, sqlx::query(&query));

            let tx_count: Option<i32> = query
                .fetch_optional(self.storage.conn())
                .await?
                .map(|db_row| db_row.get("tx_count"));

            Ok(tx_count.map(|t| (t as u32).into()))
        })
    }

    /// Returns hashes of blocks with numbers greater than `from_block` and the number of the last block.
    pub fn get_block_hashes_after(
        &mut self,
        from_block: MiniblockNumber,
        limit: usize,
    ) -> Result<(Vec<H256>, Option<MiniblockNumber>), SqlxError> {
        async_std::task::block_on(async {
            let records = sqlx::query!(
                "
                SELECT number, hash FROM miniblocks
                WHERE number > $1
                ORDER BY number ASC
                LIMIT $2
            ",
                from_block.0 as i64,
                limit as i32
            )
            .fetch_all(self.storage.conn())
            .await?;
            let last_block_number = records
                .last()
                .map(|record| MiniblockNumber(record.number as u32));
            let hashes = records
                .into_iter()
                .map(|record| H256::from_slice(&record.hash))
                .collect();
            Ok((hashes, last_block_number))
        })
    }

    /// Returns hashes of blocks with numbers greater than `from_block` and the number of the last block.
    pub fn get_block_headers_after(
        &mut self,
        from_block: MiniblockNumber,
    ) -> Result<Vec<BlockHeader>, SqlxError> {
        async_std::task::block_on(async {
            let records = sqlx::query!(
                "
                SELECT
                    hash,
                    number,
                    timestamp
                FROM miniblocks
                WHERE number > $1
                ORDER BY number ASC
            ",
                from_block.0 as i64,
            )
            .fetch_all(self.storage.conn())
            .await?;
            let blocks: Vec<BlockHeader> = records
                .into_iter()
                .map(|db_row| BlockHeader {
                    hash: Some(H256::from_slice(&db_row.hash)),
                    parent_hash: H256::zero(),
                    uncles_hash: EMPTY_UNCLES_HASH,
                    author: H160::zero(),
                    state_root: H256::zero(),
                    transactions_root: H256::zero(),
                    receipts_root: H256::zero(),
                    number: Some(U64::from(db_row.number)),
                    gas_used: Default::default(),
                    gas_limit: Default::default(),
                    base_fee_per_gas: Default::default(),
                    extra_data: Default::default(),
                    // todo logs
                    logs_bloom: Default::default(),
                    timestamp: U256::from(db_row.timestamp),
                    difficulty: Default::default(),
                    mix_hash: None,
                    nonce: None,
                })
                .collect();
            Ok(blocks)
        })
    }

    pub fn resolve_block_id(
        &mut self,
        block_id: BlockId,
    ) -> Result<Result<MiniblockNumber, Web3Error>, SqlxError> {
        async_std::task::block_on(async {
            let query_string = match block_id {
                BlockId::Hash(_) => "SELECT number FROM miniblocks WHERE hash = $1".to_string(),
                BlockId::Number(api::BlockNumber::Number(block_number)) => {
                    // The reason why instead of returning the `block_number` directly we use query is
                    // to handle numbers of blocks that are not created yet.
                    // the `SELECT number FROM miniblocks WHERE number=block_number` for
                    // non-existing block number will returns zero.
                    format!(
                        "SELECT number FROM miniblocks WHERE number = {}",
                        block_number
                    )
                }
                BlockId::Number(api::BlockNumber::Earliest) => {
                    return Ok(Ok(MiniblockNumber(0)));
                }
                BlockId::Number(block_number) => web3_block_number_to_sql(block_number),
            };
            let row = bind_block_where_sql_params(block_id, sqlx::query(&query_string))
                .fetch_optional(self.storage.conn())
                .await?;

            let block_number = row
                .and_then(|row| row.get::<Option<i64>, &str>("number"))
                .map(|n| MiniblockNumber(n as u32))
                .ok_or(Web3Error::NoBlock);
            Ok(block_number)
        })
    }

    pub fn get_block_timestamp(
        &mut self,
        block_number: MiniblockNumber,
    ) -> Result<Option<u64>, SqlxError> {
        async_std::task::block_on(async {
            let timestamp = sqlx::query!(
                r#"SELECT timestamp FROM miniblocks WHERE number = $1"#,
                block_number.0 as i64
            )
            .fetch_optional(self.storage.conn())
            .await?
            .map(|row| row.timestamp as u64);
            Ok(timestamp)
        })
    }

    pub fn get_l2_to_l1_logs(
        &mut self,
        block_number: L1BatchNumber,
    ) -> Result<Vec<L2ToL1Log>, SqlxError> {
        async_std::task::block_on(async {
            let result: Vec<Vec<u8>> = sqlx::query!(
                "SELECT l2_to_l1_logs FROM l1_batches WHERE number = $1",
                block_number.0 as i64
            )
            .fetch_optional(self.storage.conn())
            .await?
            .map(|row| row.l2_to_l1_logs)
            .unwrap_or_else(Vec::new);

            Ok(result.into_iter().map(L2ToL1Log::from).collect())
        })
    }

    pub fn get_l1_batch_number_of_miniblock(
        &mut self,
        miniblock_number: MiniblockNumber,
    ) -> Result<Option<L1BatchNumber>, SqlxError> {
        async_std::task::block_on(async {
            let number: Option<i64> = sqlx::query!(
                "
                    SELECT l1_batch_number FROM miniblocks
                    WHERE number = $1
                ",
                miniblock_number.0 as i64
            )
            .fetch_optional(self.storage.conn())
            .await?
            .and_then(|row| row.l1_batch_number);
            Ok(number.map(|number| L1BatchNumber(number as u32)))
        })
    }

    pub fn get_miniblock_range_of_l1_batch(
        &mut self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<Option<(MiniblockNumber, MiniblockNumber)>, SqlxError> {
        async_std::task::block_on(async {
            let row = sqlx::query!(
                r#"
                    SELECT MIN(miniblocks.number) as "min?", MAX(miniblocks.number) as "max?"
                    FROM miniblocks
                    WHERE l1_batch_number = $1
                "#,
                l1_batch_number.0 as i64
            )
            .fetch_one(self.storage.conn())
            .await?;
            match (row.min, row.max) {
                (Some(min), Some(max)) => Ok(Some((
                    MiniblockNumber(min as u32),
                    MiniblockNumber(max as u32),
                ))),
                (None, None) => Ok(None),
                _ => unreachable!(),
            }
        })
    }

    pub fn get_l1_batch_info_for_tx(
        &mut self,
        tx_hash: H256,
    ) -> Result<Option<(L1BatchNumber, u16)>, SqlxError> {
        async_std::task::block_on(async {
            let row = sqlx::query!(
                "
                    SELECT l1_batch_number, l1_batch_tx_index
                    FROM transactions
                    WHERE hash = $1
                ",
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
        })
    }

    pub fn get_trace_for_miniblock(&mut self, block: BlockId) -> Result<Vec<Call>, Web3Error> {
        async_std::task::block_on(async {
            let block_number = self.resolve_block_id(block).unwrap()?;
            let traces = sqlx::query_as!(
                CallTrace,
                r#"
                    SELECT * FROM call_traces WHERE tx_hash IN (
                        SELECT hash FROM transactions WHERE miniblock_number = $1
                    )
                "#,
                block_number.0 as i64
            )
            .fetch_all(self.storage.conn())
            .await
            .unwrap()
            .into_iter()
            .map(Call::from)
            .collect();
            Ok(traces)
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ConnectionPool;

    use super::*;
    use db_test_macro::db_test;
    use zksync_types::{
        api::{BlockId, BlockNumber},
        MiniblockNumber,
    };

    #[db_test(dal_crate)]
    async fn test_resolve_block_id_earliest(connection_pool: ConnectionPool) {
        let storage = &mut connection_pool.access_test_storage().await;
        let mut block_web3_dal = BlocksWeb3Dal { storage };
        let miniblock_number =
            block_web3_dal.resolve_block_id(BlockId::Number(BlockNumber::Earliest));
        assert_eq!(miniblock_number.unwrap().unwrap(), MiniblockNumber(0));
    }
}
