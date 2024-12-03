use std::{collections::HashMap, time::Duration};

use chrono::DateTime;
use serde::{Deserialize, Serialize};
use zksync_db_connection::{connection::Connection, error::DalResult, instrument::InstrumentExt};

use crate::Core;

#[derive(Debug)]
pub(crate) struct TableSize {
    pub table_size: u64,
    pub indexes_size: u64,
    pub relation_size: u64,
    pub total_size: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseMigration {
    pub version: i64,
    pub description: String,
    pub installed_on: DateTime<chrono::Utc>,
    pub success: bool,
    pub checksum: String,
    pub execution_time: Duration,
}

#[derive(Debug)]
pub struct SystemDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl SystemDal<'_, '_> {
    pub async fn get_replication_lag(&mut self) -> DalResult<Duration> {
        // NOTE: lag (seconds) has a special meaning here
        // (it is not the same that `replay_lag/write_lag/flush_lag` from `pg_stat_replication` view)
        // and it is only useful when synced column is false,
        // because lag means how many seconds elapsed since the last action was committed.
        let row = sqlx::query!(
            r#"
            SELECT
                PG_LAST_WAL_RECEIVE_LSN() = PG_LAST_WAL_REPLAY_LSN() AS synced,
                EXTRACT(
                    seconds
                    FROM
                    NOW() - PG_LAST_XACT_REPLAY_TIMESTAMP()
                )::INT AS lag
            "#
        )
        .instrument("get_replication_lag")
        .fetch_one(self.storage)
        .await?;

        Ok(match row.synced {
            Some(false) => Duration::from_secs(row.lag.unwrap_or(0) as u64),
            _ => Duration::ZERO, // We are synced, no lag
        })
    }

    pub(crate) async fn get_table_sizes(&mut self) -> DalResult<HashMap<String, TableSize>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                table_name,
                PG_TABLE_SIZE(
                    ('public.' || QUOTE_IDENT(table_name))::regclass
                ) AS table_size,
                PG_INDEXES_SIZE(
                    ('public.' || QUOTE_IDENT(table_name))::regclass
                ) AS indexes_size,
                PG_RELATION_SIZE(
                    ('public.' || QUOTE_IDENT(table_name))::regclass
                ) AS relation_size,
                PG_TOTAL_RELATION_SIZE(
                    ('public.' || QUOTE_IDENT(table_name))::regclass
                ) AS total_size
            FROM
                information_schema.tables
            WHERE
                table_schema = 'public'
            "#
        )
        .instrument("get_table_sizes")
        .report_latency()
        .expect_slow_query()
        .fetch_all(self.storage)
        .await?;

        let table_sizes = rows.into_iter().filter_map(|row| {
            Some((
                row.table_name?,
                TableSize {
                    table_size: row.table_size? as u64,
                    indexes_size: row.indexes_size.unwrap_or(0) as u64,
                    relation_size: row.relation_size.unwrap_or(0) as u64,
                    total_size: row.total_size? as u64,
                },
            ))
        });
        Ok(table_sizes.collect())
    }

    pub async fn get_last_migration(&mut self) -> DalResult<DatabaseMigration> {
        let row = sqlx::query!(
            r#"
            SELECT *
            FROM _sqlx_migrations
            ORDER BY _sqlx_migrations.version DESC
            LIMIT 1
            "#
        )
        .instrument("get_last_migration")
        .fetch_one(self.storage)
        .await?;

        Ok(DatabaseMigration {
            version: row.version,
            description: row.description,
            installed_on: row.installed_on,
            success: row.success,
            checksum: hex::encode(row.checksum),
            execution_time: Duration::from_millis(u64::try_from(row.execution_time).unwrap_or(0)),
        })
    }
}
