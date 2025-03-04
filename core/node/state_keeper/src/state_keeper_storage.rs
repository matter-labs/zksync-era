use std::collections::VecDeque;
use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::{watch, Mutex};
use zksync_dal::{ConnectionPool, Core};
use zksync_state::{
    AsyncCatchupTask, BatchDiff, OwnedStorage, ReadStorageFactory, RocksdbCell,
    RocksdbStorageOptions, BatchDiffs
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
    batch_diffs: Mutex<BatchDiffs>,
}

impl AsyncRocksdbCache {
    pub fn new(
        pool: ConnectionPool<Core>,
        state_keeper_db_path: String,
        state_keeper_db_options: RocksdbStorageOptions,
        catchup_indefinitely: bool,
    ) -> (Self, AsyncCatchupTask) {
        let (task, rocksdb_cell) =
            AsyncCatchupTask::new(pool.clone(), state_keeper_db_path, catchup_indefinitely);
        (
            Self {
                pool,
                rocksdb_cell,
                batch_diffs: Mutex::new(BatchDiffs::new()),
            },
            task.with_db_options(state_keeper_db_options),
        )
    }

    pub async fn push_batch_diff(&self, l1_batch_number: L1BatchNumber, diff: BatchDiff) {
        self.batch_diffs.lock().await.push(l1_batch_number, diff);
    }
}

#[async_trait]
impl ReadStorageFactory for AsyncRocksdbCache {
    #[tracing::instrument(skip(self, _stop_receiver))]
    async fn access_storage(
        &self,
        _stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<OwnedStorage>> {
        let initial_state = self.rocksdb_cell.ensure_initialized().await?;
        let rocksdb = if initial_state.l1_batch_number >= Some(l1_batch_number) {
            tracing::info!(
                "RocksDB cache (initial state: {initial_state:?}) doesn't need to catch up to L1 batch #{l1_batch_number}, \
                 waiting for it to become available"
            );
            // Opening the cache RocksDB can take a couple of seconds, so if we don't wait here, we unnecessarily miss an opportunity
            // to use the cache for an entire batch.
            Some(self.rocksdb_cell.wait().await?)
        } else {
            // This clause includes several cases: if the cache needs catching up or recovery, or if `l1_batch_number`
            // is not the first processed L1 batch.
            self.rocksdb_cell.get()
        };

        if let Some(rocksdb) = rocksdb {
            let mut batch_diffs_lock = self.batch_diffs.lock().await;
            let Some((storage, rocksdb_l1_batch_number)) = OwnedStorage::rocksdb_with_storage(
                rocksdb.clone(),
                &batch_diffs_lock,
                l1_batch_number,
            )
            .await
            .context("Failed accessing RocksDB storage")?
            else {
                return Ok(None);
            };

            batch_diffs_lock.trim_start(rocksdb_l1_batch_number);

            Ok(Some(storage))
        } else {
            let connection = self
                .pool
                .connection_tagged("state_keeper")
                .await
                .context("Failed getting a Postgres connection")?;
            Ok(Some(
                OwnedStorage::postgres(connection, l1_batch_number)
                    .await?
                    .into(),
            ))
        }
    }
}
