use std::time::Instant;

use sqlx::Row;

use crate::models::storage_block::web3_block_number_to_sql;
use crate::models::storage_event::StorageWeb3Log;
use crate::SqlxError;
use crate::StorageProcessor;
use zksync_types::{
    api::{self, GetLogsFilter, Log},
    MiniblockNumber,
};

#[derive(Debug)]
pub struct EventsWeb3Dal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl EventsWeb3Dal<'_, '_> {
    /// Returns miniblock number of log for given filter and offset.
    /// Used to determine if there is more than `offset` logs that satisfies filter.
    pub fn get_log_block_number(
        &mut self,
        filter: GetLogsFilter,
        offset: usize,
    ) -> Result<Option<MiniblockNumber>, SqlxError> {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            let (where_sql, arg_index) = self.build_get_logs_where_clause(&filter);

            let query = format!(
                r#"
                    SELECT miniblock_number
                    FROM events
                    WHERE {}
                    ORDER BY miniblock_number ASC, event_index_in_block ASC
                    LIMIT 1 OFFSET ${}
                "#,
                where_sql, arg_index
            );

            let mut query = sqlx::query(&query);
            query = query.bind(filter.from_block.0 as i64);

            if let Some(api::BlockNumber::Number(number)) = filter.to_block {
                query = query.bind(number.as_u64() as i64);
            }
            if !filter.addresses.is_empty() {
                let addresses: Vec<_> = filter
                    .addresses
                    .into_iter()
                    .map(|address| address.0.to_vec())
                    .collect();
                query = query.bind(addresses);
            }
            for (_, topics) in filter.topics {
                let topics: Vec<_> = topics.into_iter().map(|topic| topic.0.to_vec()).collect();
                query = query.bind(topics);
            }
            query = query.bind(offset as i32);
            let log = query.fetch_optional(self.storage.conn()).await?;

            metrics::histogram!("dal.request", started_at.elapsed(), "method" => "get_log_block_number");

            Ok(log.map(|row| MiniblockNumber(row.get::<i64, &str>("miniblock_number") as u32)))
        })
    }

    /// Returns logs for given filter.
    #[allow(clippy::type_complexity)]
    pub fn get_logs(&mut self, filter: GetLogsFilter, limit: usize) -> Result<Vec<Log>, SqlxError> {
        async_std::task::block_on(async {
            let started_at = Instant::now();
            let (where_sql, arg_index) = self.build_get_logs_where_clause(&filter);

            let query = format!(
                r#"
                WITH events_select AS (
                    SELECT
                        address, topic1, topic2, topic3, topic4, value,
                        miniblock_number, tx_hash, tx_index_in_block,
                        event_index_in_block, event_index_in_tx
                    FROM events
                    WHERE {}
                    ORDER BY miniblock_number ASC, event_index_in_block ASC
                    LIMIT ${}
                )
                SELECT miniblocks.hash as "block_hash", miniblocks.l1_batch_number as "l1_batch_number", events_select.*
                FROM events_select
                LEFT JOIN miniblocks ON events_select.miniblock_number = miniblocks.number
                ORDER BY miniblock_number ASC, event_index_in_block ASC
                "#,
                where_sql, arg_index
            );

            let mut query = sqlx::query_as(&query);
            query = query.bind(filter.from_block.0 as i64);

            if let Some(api::BlockNumber::Number(number)) = filter.to_block {
                query = query.bind(number.as_u64() as i64);
            }
            if !filter.addresses.is_empty() {
                let addresses: Vec<_> = filter
                    .addresses
                    .into_iter()
                    .map(|address| address.0.to_vec())
                    .collect();
                query = query.bind(addresses);
            }
            for (_, topics) in filter.topics {
                let topics: Vec<_> = topics.into_iter().map(|topic| topic.0.to_vec()).collect();
                query = query.bind(topics);
            }
            query = query.bind(limit as i32);

            let db_logs: Vec<StorageWeb3Log> = query.fetch_all(self.storage.conn()).await?;
            let logs = db_logs.into_iter().map(Into::into).collect();
            metrics::histogram!("dal.request", started_at.elapsed(), "method" => "get_logs");
            Ok(logs)
        })
    }

    fn build_get_logs_where_clause(&self, filter: &GetLogsFilter) -> (String, u8) {
        let mut arg_index = 1;

        let (block_sql, new_arg_index) = web3_block_number_to_sql(
            api::BlockNumber::Number(filter.from_block.0.into()),
            arg_index,
        );
        let mut where_sql = format!("(miniblock_number >= {})", block_sql);
        arg_index = new_arg_index;

        if let Some(to_block) = filter.to_block {
            let (block_sql, new_arg_index) = web3_block_number_to_sql(to_block, arg_index);
            where_sql += &format!(" AND (miniblock_number <= {})", block_sql);
            arg_index = new_arg_index;
        }
        if !filter.addresses.is_empty() {
            where_sql += &format!(" AND (address = ANY(${}))", arg_index);
            arg_index += 1;
        }
        for (topic_index, _) in filter.topics.iter() {
            where_sql += &format!(" AND (topic{} = ANY(${}))", topic_index, arg_index);
            arg_index += 1;
        }

        (where_sql, arg_index)
    }

    pub fn get_all_logs(&mut self, from_block: MiniblockNumber) -> Result<Vec<Log>, SqlxError> {
        async_std::task::block_on(async {
            let db_logs: Vec<StorageWeb3Log> = sqlx::query_as!(
                StorageWeb3Log,
                r#"
                WITH events_select AS (
                    SELECT
                        address, topic1, topic2, topic3, topic4, value,
                        miniblock_number, tx_hash, tx_index_in_block,
                        event_index_in_block, event_index_in_tx
                    FROM events
                    WHERE miniblock_number > $1
                    ORDER BY miniblock_number ASC, event_index_in_block ASC
                )
                SELECT miniblocks.hash as "block_hash?",
                    address as "address!", topic1 as "topic1!", topic2 as "topic2!", topic3 as "topic3!", topic4 as "topic4!", value as "value!",
                    miniblock_number as "miniblock_number!", miniblocks.l1_batch_number as "l1_batch_number?", tx_hash as "tx_hash!",
                    tx_index_in_block as "tx_index_in_block!", event_index_in_block as "event_index_in_block!", event_index_in_tx as "event_index_in_tx!"
                FROM events_select
                INNER JOIN miniblocks ON events_select.miniblock_number = miniblocks.number
                ORDER BY miniblock_number ASC, event_index_in_block ASC
                "#,
                from_block.0 as i64
            )
            .fetch_all(self.storage.conn())
            .await?;
            let logs = db_logs.into_iter().map(Into::into).collect();
            Ok(logs)
        })
    }
}
