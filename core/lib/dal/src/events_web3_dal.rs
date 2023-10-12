use sqlx::Row;

use zksync_types::{
    api::{GetLogsFilter, Log},
    Address, MiniblockNumber, H256,
};

use crate::{
    instrument::InstrumentExt,
    models::{storage_block::web3_block_number_to_sql, storage_event::StorageWeb3Log},
    SqlxError, StorageProcessor,
};

#[derive(Debug)]
pub struct EventsWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut StorageProcessor<'c>,
}

impl EventsWeb3Dal<'_, '_> {
    /// Returns miniblock number of log for given filter and offset.
    /// Used to determine if there is more than `offset` logs that satisfies filter.
    pub async fn get_log_block_number(
        &mut self,
        filter: &GetLogsFilter,
        offset: usize,
    ) -> Result<Option<MiniblockNumber>, SqlxError> {
        {
            let (where_sql, arg_index) = self.build_get_logs_where_clause(filter);

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

            if !filter.addresses.is_empty() {
                let addresses: Vec<_> = filter.addresses.iter().map(Address::as_bytes).collect();
                query = query.bind(addresses);
            }
            for (_, topics) in &filter.topics {
                let topics: Vec<_> = topics.iter().map(H256::as_bytes).collect();
                query = query.bind(topics);
            }
            query = query.bind(offset as i32);
            let log = query
                .instrument("get_log_block_number")
                .report_latency()
                .with_arg("filter", filter)
                .with_arg("offset", &offset)
                .fetch_optional(self.storage.conn())
                .await?;

            Ok(log.map(|row| MiniblockNumber(row.get::<i64, _>("miniblock_number") as u32)))
        }
    }

    /// Returns logs for given filter.
    #[allow(clippy::type_complexity)]
    pub async fn get_logs(
        &mut self,
        filter: GetLogsFilter,
        limit: usize,
    ) -> Result<Vec<Log>, SqlxError> {
        {
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
            if !filter.addresses.is_empty() {
                let addresses: Vec<_> = filter.addresses.iter().map(Address::as_bytes).collect();
                query = query.bind(addresses);
            }
            for (_, topics) in &filter.topics {
                let topics: Vec<_> = topics.iter().map(H256::as_bytes).collect();
                query = query.bind(topics);
            }
            query = query.bind(limit as i32);

            let db_logs: Vec<StorageWeb3Log> = query
                .instrument("get_logs")
                .report_latency()
                .with_arg("filter", &filter)
                .with_arg("limit", &limit)
                .fetch_all(self.storage.conn())
                .await?;
            let logs = db_logs.into_iter().map(Into::into).collect();
            Ok(logs)
        }
    }

    fn build_get_logs_where_clause(&self, filter: &GetLogsFilter) -> (String, u8) {
        let mut arg_index = 1;

        let mut where_sql = format!("(miniblock_number >= {})", filter.from_block.0 as i64);

        if let Some(to_block) = filter.to_block {
            let block_sql = web3_block_number_to_sql(to_block);
            where_sql += &format!(" AND (miniblock_number <= {})", block_sql);
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

    pub async fn get_all_logs(
        &mut self,
        from_block: MiniblockNumber,
    ) -> Result<Vec<Log>, SqlxError> {
        {
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
        }
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::api::BlockNumber;
    use zksync_types::{Address, H256};

    use super::*;
    use crate::connection::ConnectionPool;

    #[tokio::test]
    async fn test_build_get_logs_where_clause() {
        let connection_pool = ConnectionPool::test_pool().await;
        let storage = &mut connection_pool.access_storage().await.unwrap();
        let events_web3_dal = EventsWeb3Dal { storage };
        let filter = GetLogsFilter {
            from_block: MiniblockNumber(100),
            to_block: Some(BlockNumber::Number(200.into())),
            addresses: vec![Address::from_low_u64_be(123)],
            topics: vec![(0, vec![H256::from_low_u64_be(456)])],
        };

        let expected_sql = "(miniblock_number >= 100) AND (miniblock_number <= 200) AND (address = ANY($1)) AND (topic0 = ANY($2))";
        let expected_arg_index = 3;

        let (actual_sql, actual_arg_index) = events_web3_dal.build_get_logs_where_clause(&filter);

        assert_eq!(actual_sql, expected_sql);
        assert_eq!(actual_arg_index, expected_arg_index);
    }
}
