use sqlx::{
    postgres::PgArguments,
    query::{Query, QueryAs},
    Postgres, Row,
};
use zksync_types::{
    api::{GetLogsFilter, Log},
    Address, MiniblockNumber, H256,
};

use crate::{
    instrument::InstrumentExt, models::storage_event::StorageWeb3Log, SqlxError, StorageProcessor,
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

            // Bind address params - noop if there are no addresses
            query = Self::bind_params_for_optional_filter_query(
                query,
                filter.addresses.iter().map(Address::as_bytes).collect(),
            );
            for (_, topics) in &filter.topics {
                // Bind topic params - noop if there are no topics
                query = Self::bind_params_for_optional_filter_query(
                    query,
                    topics.iter().map(H256::as_bytes).collect(),
                );
            }
            query = query.bind(offset as i32);
            let log = query
                .instrument("get_log_block_number")
                .report_latency()
                .with_arg("filter", filter)
                .with_arg("offset", &offset)
                .fetch_optional(self.storage)
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

            // Bind address params - noop if there are no addresses
            query = Self::bind_params_for_optional_filter_query_as(
                query,
                filter.addresses.iter().map(Address::as_bytes).collect(),
            );
            for (_, topics) in &filter.topics {
                // Bind topic params - noop if there are no topics
                query = Self::bind_params_for_optional_filter_query_as(
                    query,
                    topics.iter().map(H256::as_bytes).collect(),
                );
            }
            query = query.bind(limit as i32);

            let db_logs: Vec<StorageWeb3Log> = query
                .instrument("get_logs")
                .report_latency()
                .with_arg("filter", &filter)
                .with_arg("limit", &limit)
                .fetch_all(self.storage)
                .await?;
            let logs = db_logs.into_iter().map(Into::into).collect();
            Ok(logs)
        }
    }

    fn build_get_logs_where_clause(&self, filter: &GetLogsFilter) -> (String, u8) {
        let mut arg_index = 1;

        let mut where_sql = format!("(miniblock_number >= {})", filter.from_block.0 as i64);

        where_sql += &format!(" AND (miniblock_number <= {})", filter.to_block.0 as i64);

        // Add filters for address (like `address = ANY($1)` or `address = $1`)
        if let Some(filter_sql) =
            Self::build_sql_filter(filter.addresses.len() as u32, "address", arg_index)
        {
            where_sql += &filter_sql;
            arg_index += 1;
        }

        // Add filters for topics (like `topic0 = ANY($2)`)
        for (topic_index, topics) in filter.topics.iter() {
            if let Some(filter_sql) = Self::build_sql_filter(
                topics.len() as u32,
                &format!("topic{}", topic_index),
                arg_index,
            ) {
                where_sql += &filter_sql;
                arg_index += 1;
            }
        }

        (where_sql, arg_index)
    }

    // Builds SQL filter for optional filter (like address or topics).
    fn build_sql_filter(
        number_of_entities: u32,
        field_name: &str,
        arg_index: u8,
    ) -> Option<String> {
        match number_of_entities {
            0 => None,
            1 => Some(format!(" AND ({} = ${})", field_name, arg_index)),
            _ => Some(format!(" AND ({} = ANY(${}))", field_name, arg_index)),
        }
    }

    // Binds parameters for optional filter (like address or topics).
    // Noop if there are no values.
    // Assumes `=$1` syntax for single value and `=ANY($1)` for multiple values.
    // See the method above for details.
    fn bind_params_for_optional_filter_query_as<'q, O>(
        query: QueryAs<'q, Postgres, O, PgArguments>,
        values: Vec<&'q [u8]>,
    ) -> QueryAs<'q, Postgres, O, PgArguments> {
        match values.len() {
            0 => query,
            1 => query.bind(values[0]),
            _ => query.bind(values),
        }
    }

    // Same as `bind_params_for_optional_filter_query_as` but for `Query` instead of `QueryAs`.
    fn bind_params_for_optional_filter_query<'q>(
        query: Query<'q, Postgres, PgArguments>,
        values: Vec<&'q [u8]>,
    ) -> Query<'q, Postgres, PgArguments> {
        match values.len() {
            0 => query,
            1 => query.bind(values[0]),
            _ => query.bind(values),
        }
    }

    pub async fn get_all_logs(
        &mut self,
        from_block: MiniblockNumber,
    ) -> Result<Vec<Log>, SqlxError> {
        {
            let db_logs: Vec<StorageWeb3Log> = sqlx::query_as!(
                StorageWeb3Log,
                r#"
                WITH
                    events_select AS (
                        SELECT
                            address,
                            topic1,
                            topic2,
                            topic3,
                            topic4,
                            value,
                            miniblock_number,
                            tx_hash,
                            tx_index_in_block,
                            event_index_in_block,
                            event_index_in_tx
                        FROM
                            events
                        WHERE
                            miniblock_number > $1
                        ORDER BY
                            miniblock_number ASC,
                            event_index_in_block ASC
                    )
                SELECT
                    miniblocks.hash AS "block_hash?",
                    address AS "address!",
                    topic1 AS "topic1!",
                    topic2 AS "topic2!",
                    topic3 AS "topic3!",
                    topic4 AS "topic4!",
                    value AS "value!",
                    miniblock_number AS "miniblock_number!",
                    miniblocks.l1_batch_number AS "l1_batch_number?",
                    tx_hash AS "tx_hash!",
                    tx_index_in_block AS "tx_index_in_block!",
                    event_index_in_block AS "event_index_in_block!",
                    event_index_in_tx AS "event_index_in_tx!"
                FROM
                    events_select
                    INNER JOIN miniblocks ON events_select.miniblock_number = miniblocks.number
                ORDER BY
                    miniblock_number ASC,
                    event_index_in_block ASC
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
            to_block: MiniblockNumber(200),
            addresses: vec![Address::from_low_u64_be(123)],
            topics: vec![(0, vec![H256::from_low_u64_be(456)])],
        };

        let expected_sql = "(miniblock_number >= 100) AND (miniblock_number <= 200) AND (address = $1) AND (topic0 = $2)";
        let expected_arg_index = 3;

        let (actual_sql, actual_arg_index) = events_web3_dal.build_get_logs_where_clause(&filter);

        assert_eq!(actual_sql, expected_sql);
        assert_eq!(actual_arg_index, expected_arg_index);
    }

    #[tokio::test]
    async fn test_build_get_logs_with_multiple_topics_where_clause() {
        let connection_pool = ConnectionPool::test_pool().await;
        let storage = &mut connection_pool.access_storage().await.unwrap();
        let events_web3_dal = EventsWeb3Dal { storage };
        let filter = GetLogsFilter {
            from_block: MiniblockNumber(10),
            to_block: MiniblockNumber(400),
            addresses: vec![
                Address::from_low_u64_be(123),
                Address::from_low_u64_be(1233),
            ],
            topics: vec![
                (
                    0,
                    vec![
                        H256::from_low_u64_be(456),
                        H256::from_low_u64_be(789),
                        H256::from_low_u64_be(101),
                    ],
                ),
                (2, vec![H256::from_low_u64_be(789)]),
            ],
        };

        let expected_sql = "(miniblock_number >= 10) AND (miniblock_number <= 400) AND (address = ANY($1)) AND (topic0 = ANY($2)) AND (topic2 = $3)";
        let expected_arg_index = 4;

        let (actual_sql, actual_arg_index) = events_web3_dal.build_get_logs_where_clause(&filter);

        assert_eq!(actual_sql, expected_sql);
        assert_eq!(actual_arg_index, expected_arg_index);
    }

    #[tokio::test]
    async fn test_build_get_logs_with_no_address_where_clause() {
        let connection_pool = ConnectionPool::test_pool().await;
        let storage = &mut connection_pool.access_storage().await.unwrap();
        let events_web3_dal = EventsWeb3Dal { storage };
        let filter = GetLogsFilter {
            from_block: MiniblockNumber(10),
            to_block: MiniblockNumber(400),
            addresses: vec![],
            topics: vec![(2, vec![H256::from_low_u64_be(789)])],
        };

        let expected_sql =
            "(miniblock_number >= 10) AND (miniblock_number <= 400) AND (topic2 = $1)";
        let expected_arg_index = 2;

        let (actual_sql, actual_arg_index) = events_web3_dal.build_get_logs_where_clause(&filter);

        assert_eq!(actual_sql, expected_sql);
        assert_eq!(actual_arg_index, expected_arg_index);
    }
}
