use std::{fmt::Debug, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_state::{
    AsyncCatchupTask, PgOrRocksdbStorage, ReadStorageFactory, RocksdbStorageOptions,
    StateKeeperColumnFamily,
};
use zksync_storage::RocksDB;
use zksync_types::L1BatchNumber;

/// A [`ReadStorageFactory`] implementation that can produce short-lived [`ReadStorage`] handles
/// backed by either Postgres or RocksDB (if it's caught up). Always initialized as a `Postgres`
/// variant and is then mutated into `Rocksdb` once RocksDB cache is caught up. After which it
/// can never revert back to `Postgres` as we assume RocksDB cannot fall behind under normal state
/// keeper operation.
#[derive(Debug, Clone)]
pub struct AsyncRocksdbCache {
    pool: ConnectionPool<Core>,
    rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
}

impl AsyncRocksdbCache {
    async fn access_storage_inner(
        &self,
        stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        if let Some(rocksdb) = self.rocksdb_cell.get() {
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
        let rocksdb_cell = Arc::new(OnceCell::new());
        let task = AsyncCatchupTask::new(
            pool.clone(),
            state_keeper_db_path,
            state_keeper_db_options,
            rocksdb_cell.clone(),
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
