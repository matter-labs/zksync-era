use std::collections::HashMap;

use sqlx::Row;
use zksync_db_connection::{connection::Connection, instrument::InstrumentExt};

#[derive(Debug)]
pub(crate) struct TableSize {
    pub table_size: u64,
    pub indexes_size: u64,
    pub relation_size: u64,
    pub total_size: u64,
}
use crate::Core;

pub struct SystemDal<'a, 'c> {
    pub storage: &'a mut Connection<'c, Core>,
}

impl SystemDal<'_, '_> {
    pub async fn get_replication_lag_sec(&mut self) -> u32 {
        // NOTE: lag (seconds) has a special meaning here
        // (it is not the same that `replay_lag/write_lag/flush_lag` from `pg_stat_replication` view)
        // and it is only useful when synced column is false,
        // because lag means how many seconds elapsed since the last action was committed.
        let pg_row = sqlx::query(
            "SELECT \
                 pg_last_wal_receive_lsn() = pg_last_wal_replay_lsn() AS synced, \
                 EXTRACT(SECONDS FROM now() - pg_last_xact_replay_timestamp())::int AS lag",
        )
        .fetch_one(self.storage.conn())
        .await
        .unwrap();

        match pg_row.get("synced") {
            Some(false) => pg_row.try_get::<i64, &str>("lag").unwrap_or_default() as u32,
            // We are synced, no lag
            _ => 0,
        }
    }

    pub(crate) async fn get_table_sizes(&mut self) -> sqlx::Result<HashMap<String, TableSize>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                table_name,
                PG_TABLE_SIZE(('public.' || QUOTE_IDENT(table_name))::regclass) AS table_size,
                PG_INDEXES_SIZE(('public.' || QUOTE_IDENT(table_name))::regclass) AS indexes_size,
                PG_RELATION_SIZE(('public.' || QUOTE_IDENT(table_name))::regclass) AS relation_size,
                PG_TOTAL_RELATION_SIZE(('public.' || QUOTE_IDENT(table_name))::regclass) AS total_size
            FROM
                information_schema.tables
            WHERE
                table_schema = 'public'
            "#
        )
        .instrument("get_table_sizes")
        .report_latency()
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
}
