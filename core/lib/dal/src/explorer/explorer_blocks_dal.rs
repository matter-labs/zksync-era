use zksync_types::explorer_api::{BlockDetails, BlockPageItem, BlocksQuery, PaginationDirection};
use zksync_types::MiniblockNumber;

use crate::models::storage_block::{block_page_item_from_storage, StorageBlockDetails};
use crate::SqlxError;
use crate::StorageProcessor;

#[derive(Debug)]
pub struct ExplorerBlocksDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl ExplorerBlocksDal<'_, '_> {
    pub fn get_blocks_page(
        &mut self,
        query: BlocksQuery,
        last_verified: MiniblockNumber,
    ) -> Result<Vec<BlockPageItem>, SqlxError> {
        async_std::task::block_on(async {
            let (cmp_sign, order_str) = match query.pagination.direction {
                PaginationDirection::Older => ("<", "DESC"),
                PaginationDirection::Newer => (">", "ASC"),
            };
            let cmp_str = if query.from.is_some() {
                format!("WHERE miniblocks.number {} $3", cmp_sign)
            } else {
                "".to_string()
            };
            let sql_query_str = format!(
                "
                SELECT number, l1_tx_count, l2_tx_count, hash, timestamp FROM miniblocks
                {}
                ORDER BY miniblocks.number {}
                LIMIT $1
                OFFSET $2
                ",
                cmp_str, order_str
            );

            let mut sql_query = sqlx::query_as(&sql_query_str).bind(query.pagination.limit as i32);
            sql_query = sql_query.bind(query.pagination.offset as i32);
            if let Some(from) = query.from {
                sql_query = sql_query.bind(from.0 as i64);
            }
            let result = sql_query
                .fetch_all(self.storage.conn())
                .await?
                .into_iter()
                .map(|row| block_page_item_from_storage(row, last_verified))
                .collect();
            Ok(result)
        })
    }

    pub fn get_block_details(
        &mut self,
        block_number: MiniblockNumber,
    ) -> Result<Option<BlockDetails>, SqlxError> {
        async_std::task::block_on(async {
            let block_details: Option<StorageBlockDetails> = sqlx::query_as!(
                StorageBlockDetails,
                r#"
                    SELECT miniblocks.number,
                        miniblocks.timestamp,
                        miniblocks.l1_tx_count,
                        miniblocks.l2_tx_count,
                        miniblocks.hash as "root_hash?",
                        commit_tx.tx_hash as "commit_tx_hash?",
                        commit_tx.confirmed_at as "committed_at?",
                        prove_tx.tx_hash as "prove_tx_hash?",
                        prove_tx.confirmed_at as "proven_at?",
                        execute_tx.tx_hash as "execute_tx_hash?",
                        execute_tx.confirmed_at as "executed_at?"
                    FROM miniblocks
                    LEFT JOIN l1_batches ON miniblocks.l1_batch_number = l1_batches.number
                    LEFT JOIN eth_txs_history as commit_tx ON (l1_batches.eth_commit_tx_id = commit_tx.eth_tx_id AND commit_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs_history as prove_tx ON (l1_batches.eth_prove_tx_id = prove_tx.eth_tx_id AND prove_tx.confirmed_at IS NOT NULL)
                    LEFT JOIN eth_txs_history as execute_tx ON (l1_batches.eth_execute_tx_id = execute_tx.eth_tx_id AND execute_tx.confirmed_at IS NOT NULL)
                    WHERE miniblocks.number = $1
                "#,
                block_number.0 as i64
            )
            .fetch_optional(self.storage.conn())
            .await?;
            Ok(block_details.map(BlockDetails::from))
        })
    }
}
