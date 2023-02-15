use zksync_types::api::Log;
use zksync_types::explorer_api::{EventsQuery, EventsResponse, PaginationDirection};

use sqlx::Row;

use crate::models::storage_event::StorageWeb3Log;
use crate::{SqlxError, StorageProcessor};

#[derive(Debug)]
pub struct ExplorerEventsDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl ExplorerEventsDal<'_, '_> {
    pub fn get_events_page(
        &mut self,
        query: EventsQuery,
        max_total: usize,
    ) -> Result<EventsResponse, SqlxError> {
        async_std::task::block_on(async {
            let (cmp_sign, order_str) = match query.pagination.direction {
                PaginationDirection::Older => ("<", "DESC"),
                PaginationDirection::Newer => (">", "ASC"),
            };

            let mut filters = Vec::new();
            let mut bind_index = 1usize;
            if query.from_block_number.is_some() {
                filters.push(format!(
                    "(events.miniblock_number {} ${})",
                    cmp_sign, bind_index
                ));
                bind_index += 1;
            }
            if query.contract_address.is_some() {
                filters.push(format!("(events.address = ${})", bind_index));
                bind_index += 1;
            }
            let filters: String = if !filters.is_empty() {
                format!("WHERE {}", filters.join(" AND "))
            } else {
                "".to_string()
            };

            let ordering = format!(
                "events.miniblock_number {0}, events.event_index_in_block {0}",
                order_str
            );
            let sql_list_query_str = format!(
                r#"
                SELECT events.*, miniblocks.hash as "block_hash", miniblocks.l1_batch_number
                FROM (
                    SELECT address, topic1, topic2, topic3, topic4, value,
                        miniblock_number, tx_hash, tx_index_in_block,
                        event_index_in_block, event_index_in_tx
                    FROM events
                    {0}
                    ORDER BY {1}
                    LIMIT ${2}
                    OFFSET ${3}
                ) as events
                JOIN miniblocks ON events.miniblock_number = miniblocks.number
                ORDER BY {1}
                "#,
                filters,
                ordering,
                bind_index,
                bind_index + 1
            );

            let mut sql_query = sqlx::query_as(&sql_list_query_str);
            if let Some(block_number) = query.from_block_number {
                sql_query = sql_query.bind(block_number.0 as i64);
            }
            if let Some(contract_address) = query.contract_address {
                sql_query = sql_query.bind(contract_address.0.to_vec());
            }
            sql_query = sql_query
                .bind(query.pagination.limit as i64)
                .bind(query.pagination.offset as i64);

            let storage_web3_logs: Vec<StorageWeb3Log> =
                sql_query.fetch_all(self.storage.conn()).await?;
            let logs = storage_web3_logs.into_iter().map(Log::from).collect();

            let sql_count_query_str = format!(
                r#"
                SELECT COUNT(*) as "count" FROM (
                    SELECT true
                    FROM events
                    {0}
                    LIMIT ${1}
                ) AS c
                "#,
                filters, bind_index
            );

            let mut sql_query = sqlx::query(&sql_count_query_str);
            if let Some(block_number) = query.from_block_number {
                sql_query = sql_query.bind(block_number.0 as i64);
            }
            if let Some(contract_address) = query.contract_address {
                sql_query = sql_query.bind(contract_address.0.to_vec());
            }
            sql_query = sql_query.bind(max_total as i64);

            let total = sql_query
                .fetch_one(self.storage.conn())
                .await?
                .get::<i64, &str>("count");
            Ok(EventsResponse {
                list: logs,
                total: total as usize,
            })
        })
    }
}
