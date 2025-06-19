use std::time::Duration;

use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::{watch, Mutex};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_state::{
    AsyncCatchupTask, BatchDiff, BatchDiffs, KeepUpdatedTask, OwnedStorage, ReadStorageFactory,
    RocksdbCell, RocksdbStorage, RocksdbStorageOptions,
};
use zksync_types::{L1BatchNumber, H256};

use crate::io::IoCursor;

/// A [`ReadStorageFactory`] implementation that can produce short-lived [`ReadStorage`] handles
/// backed by either Postgres or RocksDB (if it's caught up). Always initialized as a `Postgres`
/// variant and is then mutated into `Rocksdb` once RocksDB cache is caught up. After which it
/// can never revert back to `Postgres` as we assume RocksDB cannot fall behind under normal state
/// keeper operation.
#[derive(Debug)]
pub struct ZkOsAsyncRocksdbCache {
    pool: ConnectionPool<Core>,
    rocksdb_cell: RocksdbCell,
    batch_diffs: Mutex<BatchDiffs>,
}

impl ZkOsAsyncRocksdbCache {
    pub fn new(
        pool: ConnectionPool<Core>,
        state_keeper_db_path: String,
        state_keeper_db_options: RocksdbStorageOptions,
    ) -> (Self, AsyncCatchupTask, KeepUpdatedTask) {
        let (catchup_task, rocksdb_cell) =
            AsyncCatchupTask::new(pool.clone(), state_keeper_db_path);
        let keep_updated_task = rocksdb_cell.keep_updated(pool.clone());
        (
            Self {
                pool,
                rocksdb_cell,
                batch_diffs: Mutex::new(BatchDiffs::new()),
            },
            catchup_task.with_db_options(state_keeper_db_options),
            keep_updated_task,
        )
    }

    pub async fn push_batch_diff(&self, l1_batch_number: L1BatchNumber, diff: BatchDiff) {
        self.batch_diffs.lock().await.push(l1_batch_number, diff);
    }

    pub async fn next_enum_index(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<u64>> {
        let mut connection = self.pool.connection_tagged("state_keeper").await?;
        // First we check if data is present in postgres.
        let cursor = IoCursor::new(&mut connection).await?;
        if cursor.l1_batch > l1_batch_number {
            let next_enum_index = connection
                .storage_logs_dedup_dal()
                .max_enumeration_index_by_l1_batch(l1_batch_number)
                .await?
                .context("missing enum index in DB")?
                + 1;
            return Ok(Some(next_enum_index));
        }

        // Second we check if index can be deducted from DB and batch diffs.
        let batch_diffs = self.batch_diffs.lock().await;
        if batch_diffs
            .last_l1_batch_number()
            .is_some_and(|last_l1_batch_number| last_l1_batch_number >= l1_batch_number)
        {
            let first_batch_in_diffs = batch_diffs
                .first_diff_l1_batch_number()
                .expect("Diff should not be empty");
            let max_enum_index_from_postgres = connection
                .storage_logs_dedup_dal()
                .max_enumeration_index_by_l1_batch(first_batch_in_diffs - 1)
                .await?
                .context("missing enum index in DB")?;
            let initial_writes_in_diff =
                batch_diffs.number_of_initial_writes_up_to(l1_batch_number);
            Ok(Some(
                max_enum_index_from_postgres + initial_writes_in_diff + 1,
            ))
        } else {
            Ok(None)
        }
    }

    pub async fn extract_initial_writes(
        &self,
        written_keys: &[H256],
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<Vec<H256>>> {
        let mut connection = self.pool.connection_tagged("state_keeper").await?;
        // First we check if data is present in postgres.
        let cursor = IoCursor::new(&mut connection).await?;
        if cursor.l1_batch > l1_batch_number {
            let non_initial_writes = connection
                .storage_logs_dedup_dal()
                .filter_written_slots(written_keys)
                .await?;
            return Ok(Some(
                written_keys
                    .iter()
                    .filter(|hashed_key| !non_initial_writes.contains(hashed_key))
                    .copied()
                    .collect(),
            ));
        }

        // Second we check if initial writes can be deducted from DB and batch diffs.
        let batch_diffs = self.batch_diffs.lock().await;
        if batch_diffs
            .last_l1_batch_number()
            .is_some_and(|last_l1_batch_number| last_l1_batch_number >= l1_batch_number)
        {
            let non_initial_writes_from_postgres = connection
                .storage_logs_dedup_dal()
                .filter_written_slots(written_keys)
                .await?;

            let non_initial_writes_from_diffs =
                batch_diffs.filter_written_slots_up_to(l1_batch_number, written_keys);

            Ok(Some(
                written_keys
                    .iter()
                    .filter(|hashed_key| {
                        !non_initial_writes_from_postgres.contains(hashed_key)
                            && !non_initial_writes_from_diffs.contains(hashed_key)
                    })
                    .copied()
                    .collect(),
            ))
        } else {
            Ok(None)
        }
    }
}

#[async_trait]
impl ReadStorageFactory for ZkOsAsyncRocksdbCache {
    #[tracing::instrument(skip(self, _stop_receiver))]
    async fn access_storage(
        &self,
        _stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<OwnedStorage>> {
        let initial_state = self.rocksdb_cell.ensure_initialized().await?;
        let rocksdb = if initial_state
            .next_l1_batch_number
            .unwrap_or(L1BatchNumber(0))
            >= l1_batch_number
        {
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
            let rocksdb: RocksdbStorage = rocksdb.clone();
            let rocksdb_l1_batch_number = rocksdb.next_l1_batch_number().await;

            let storage =
                OwnedStorage::rocksdb_with_memory(rocksdb, &batch_diffs_lock, l1_batch_number)
                    .await
                    .context("Failed accessing RocksDB storage")?;

            batch_diffs_lock.trim_start(rocksdb_l1_batch_number);

            Ok(Some(storage))
        } else {
            let mut connection = self
                .pool
                .connection_tagged("state_keeper")
                .await
                .context("Failed getting a Postgres connection")?;
            // Block data is sealed asynchronously from block execution, we should wait for it.
            loop {
                let cursor = IoCursor::new(&mut connection).await?;
                if cursor.l1_batch == l1_batch_number + 1 {
                    break;
                }

                tracing::info!("Waiting for batch sealing to obtain postgres-based storage");
                tokio::time::sleep(Duration::from_millis(10)).await;
            }

            Ok(Some(
                OwnedStorage::postgres(connection, l1_batch_number)
                    .await?
                    .into(),
            ))
        }
    }
}
