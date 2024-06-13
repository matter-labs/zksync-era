use std::fmt::Debug;

use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_state::{
    AsyncCatchupTask, PgOrRocksdbStorage, ReadStorageFactory, RocksdbCell, RocksdbStorageOptions,
};
use zksync_types::L1BatchNumber;

/// A [`ReadStorageFactory`] implementation that can produce short-lived [`ReadStorage`] handles
/// backed by either Postgres or RocksDB (if it's caught up). Always initialized as a `Postgres`
/// variant and is then mutated into `Rocksdb` once RocksDB cache is caught up. After which it
/// can never revert back to `Postgres` as we assume RocksDB cannot fall behind under normal state
/// keeper operation.
#[derive(Debug)]
pub struct AsyncRocksdbCache {
    pool: ConnectionPool<Core>,
    rocksdb_cell: RocksdbCell,
}

impl AsyncRocksdbCache {
    async fn access_storage_inner(
        &self,
        stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        let initial_state = self.rocksdb_cell.ensure_initialized().await?;
        let rocksdb = if initial_state.l1_batch_number == Some(l1_batch_number) {
            // RocksDB cache doesn't need to catch up; wait until the cell is set (which should be quite soon)
            // to not miss the opportunity to use the cache
            Some(self.rocksdb_cell.wait().await?)
        } else {
            self.rocksdb_cell.get()
        };

        if let Some(rocksdb) = rocksdb {
            let mut connection = self
                .pool
                .connection_tagged("state_keeper")
                .await
                .context("Failed getting a Postgres connection")?;
            PgOrRocksdbStorage::access_storage_rocksdb(
                &mut connection,
                rocksdb.clone(),
                stop_receiver,
                l1_batch_number,
            )
            .await
            .context("Failed accessing RocksDB storage")
        } else {
            Ok(Some(
                PgOrRocksdbStorage::access_storage_pg(&self.pool, l1_batch_number)
                    .await
                    .context("Failed accessing Postgres storage")?,
            ))
        }
    }

    pub fn new(
        pool: ConnectionPool<Core>,
        state_keeper_db_path: String,
        state_keeper_db_options: RocksdbStorageOptions,
    ) -> (Self, AsyncCatchupTask) {
        let (task, rocksdb_cell) = AsyncCatchupTask::new(
            pool.clone(),
            state_keeper_db_path,
            state_keeper_db_options,
            None,
        );
        (Self { pool, rocksdb_cell }, task)
    }
}

#[async_trait]
impl ReadStorageFactory for AsyncRocksdbCache {
    async fn access_storage(
        &self,
        stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        self.access_storage_inner(stop_receiver, l1_batch_number)
            .await
    }
}
