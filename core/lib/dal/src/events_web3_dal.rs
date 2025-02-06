use sqlx::{
    postgres::PgArguments,
    query::{Query, QueryAs},
    Postgres, Row,
};
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};
use zksync_system_constants::CONTRACT_DEPLOYER_ADDRESS;
use zksync_types::{
    api::{GetLogsFilter, Log},
    h256_to_address, Address, L2BlockNumber, H256,
};
use zksync_vm_interface::VmEvent;

use crate::{models::storage_event::StorageWeb3Log, Core};

#[derive(Debug, PartialEq)]
pub struct ContractDeploymentLog {
    pub transaction_index_in_block: u64,
    pub deployer: Address,
    pub deployed_address: Address,
}

#[derive(Debug)]
pub struct EventsWeb3Dal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl EventsWeb3Dal<'_, '_> {
    /// Returns L2 block number of log for given filter and offset.
    /// Used to determine if there is more than `offset` logs that satisfies filter.
    pub async fn get_log_block_number(
        &mut self,
        filter: &GetLogsFilter,
        offset: usize,
    ) -> DalResult<Option<L2BlockNumber>> {
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

        Ok(log.map(|row| L2BlockNumber(row.get::<i64, _>("miniblock_number") as u32)))
    }

    /// Returns logs for given filter.
    #[allow(clippy::type_complexity)]
    pub async fn get_logs(&mut self, filter: GetLogsFilter, limit: usize) -> DalResult<Vec<Log>> {
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
            SELECT miniblocks.hash as "block_hash", miniblocks.l1_batch_number as "l1_batch_number",
                miniblocks.timestamp as block_timestamp, events_select.*
            FROM events_select
            INNER JOIN miniblocks ON events_select.miniblock_number = miniblocks.number
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

    fn build_get_logs_where_clause(&self, filter: &GetLogsFilter) -> (String, u8) {
        let mut arg_index = 1;

        let mut where_sql = format!("(miniblock_number >= {})", filter.from_block.0);
        where_sql += &format!(" AND (miniblock_number <= {})", filter.to_block.0);

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

    pub async fn get_all_logs(&mut self, from_block: L2BlockNumber) -> DalResult<Vec<Log>> {
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
                event_index_in_tx AS "event_index_in_tx!",
                miniblocks.timestamp AS "block_timestamp"
            FROM
                events_select
            INNER JOIN miniblocks ON events_select.miniblock_number = miniblocks.number
            ORDER BY
                miniblock_number ASC,
                event_index_in_block ASC
            "#,
            i64::from(from_block.0)
        )
        .instrument("get_all_logs")
        .with_arg("from_block", &from_block)
        .fetch_all(self.storage)
        .await?;
        let logs = db_logs.into_iter().map(Into::into).collect();
        Ok(logs)
    }

    /// Gets all contract deployment logs for the specified block. The returned logs are ordered by their execution order.
    pub async fn get_contract_deployment_logs(
        &mut self,
        block: L2BlockNumber,
    ) -> DalResult<Vec<ContractDeploymentLog>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                tx_index_in_block AS "transaction_index!",
                topic2 AS "deployer!",
                topic4 AS "deployed_address!"
            FROM events
            WHERE miniblock_number = $1 AND address = $2 AND topic1 = $3
            ORDER BY event_index_in_block
            "#,
            i64::from(block.0),
            CONTRACT_DEPLOYER_ADDRESS.as_bytes(),
            VmEvent::DEPLOY_EVENT_SIGNATURE.as_bytes()
        )
        .instrument("get_contract_deployment_logs")
        .with_arg("block", &block)
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| ContractDeploymentLog {
                transaction_index_in_block: row.transaction_index as u64,
                deployer: h256_to_address(&H256::from_slice(&row.deployer)),
                deployed_address: h256_to_address(&H256::from_slice(&row.deployed_address)),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{Address, H256};

    use super::*;
    use crate::{ConnectionPool, Core};

    #[tokio::test]
    async fn test_build_get_logs_where_clause() {
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let storage = &mut connection_pool.connection().await.unwrap();
        let events_web3_dal = EventsWeb3Dal { storage };
        let filter = GetLogsFilter {
            from_block: L2BlockNumber(100),
            to_block: L2BlockNumber(200),
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
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let storage = &mut connection_pool.connection().await.unwrap();
        let events_web3_dal = EventsWeb3Dal { storage };
        let filter = GetLogsFilter {
            from_block: L2BlockNumber(10),
            to_block: L2BlockNumber(400),
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
        let connection_pool = ConnectionPool::<Core>::test_pool().await;
        let storage = &mut connection_pool.connection().await.unwrap();
        let events_web3_dal = EventsWeb3Dal { storage };
        let filter = GetLogsFilter {
            from_block: L2BlockNumber(10),
            to_block: L2BlockNumber(400),
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
