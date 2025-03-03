use std::{fmt::Debug, sync::atomic::AtomicU32};

use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::{watch, RwLock};
use zksync_dal::{ConnectionPool, Core};
use zksync_state::{
    AsyncCatchupTask, BatchDiff, OwnedStorage, ReadStorageFactory, RocksdbCell,
    RocksdbStorageOptions,
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
    batch_diffs: RwLock<(Vec<BatchDiff>, Option<L1BatchNumber>)>,
}

impl AsyncRocksdbCache {
    pub fn new(
        pool: ConnectionPool<Core>,
        state_keeper_db_path: String,
        state_keeper_db_options: RocksdbStorageOptions,
    ) -> (Self, AsyncCatchupTask) {
        let (task, rocksdb_cell) = AsyncCatchupTask::new(pool.clone(), state_keeper_db_path);
        (
            Self {
                pool,
                rocksdb_cell,
                batch_diffs: Default::default(),
            },
            task.with_db_options(state_keeper_db_options),
        )
    }

    pub async fn push_batch_diff(&self, l1_batch_number: L1BatchNumber, diff: BatchDiff) {
        let mut lock = self.batch_diffs.write().await;
        lock.0.push(diff);
        lock.1 = Some(l1_batch_number);
    }
}

#[async_trait]
impl ReadStorageFactory for AsyncRocksdbCache {
    #[tracing::instrument(skip(self, stop_receiver))]
    async fn access_storage(
        &self,
        stop_receiver: &watch::Receiver<bool>,
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

        let mut connection = self
            .pool
            .connection_tagged("state_keeper")
            .await
            .context("Failed getting a Postgres connection")?;
        if let Some(rocksdb) = rocksdb {
            let mut lock = self.batch_diffs.write().await;
            let Some((storage, rocksdb_l1_batch_number)) = OwnedStorage::rocksdb(
                &mut connection,
                rocksdb.clone(),
                &lock.0,
                lock.1,
                stop_receiver,
                l1_batch_number,
            )
            .await
            .context("Failed accessing RocksDB storage")?
            else {
                return Ok(None);
            };

            if let Some(first_diff_batch_number) = lock.1 {
                if first_diff_batch_number < rocksdb_l1_batch_number {
                    for _ in 0..(rocksdb_l1_batch_number.0 - first_diff_batch_number.0) {
                        lock.0.remove(0);
                    }
                }
            }

            Ok(Some(storage))
        } else {
            Ok(Some(
                OwnedStorage::postgres(connection, l1_batch_number)
                    .await?
                    .into(),
            ))
        }
    }
}
