use sqlx::Row;

use crate::StorageProcessor;

pub struct SystemDal<'a, 'c> {
    pub storage: &'a mut StorageProcessor<'c>,
}

impl SystemDal<'_, '_> {
    pub async fn get_replication_lag_sec(&mut self) -> u32 {
        // NOTE: lag (seconds) has a special meaning here
        // (it is not the same that replay_lag/write_lag/flush_lag from pg_stat_replication view)
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
}
