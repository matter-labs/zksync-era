use std::{
    error, fmt,
    time::{Duration, Instant},
};

use anyhow::Context;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_shared_metrics::{SnapshotRecoveryStage, APP_METRICS};
use zksync_types::L1BatchNumber;

use crate::{rocksdb::InitStrategy, RocksdbStorage, RocksdbStorageOptions};

/// Initial RocksDB cache state returned by [`RocksdbCell::ensure_initialized()`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InitialRocksdbState {
    /// Last processed L1 batch number in the RocksDB cache + 1 (i.e., the batch that the cache is ready to process).
    /// `None` if the cache is empty (i.e., needs recovery).
    pub next_l1_batch_number: Option<L1BatchNumber>,
}

/// Error returned from [`RocksdbCell`] methods if the corresponding [`AsyncCatchupTask`] has failed
/// or was canceled.
#[derive(Debug)]
pub struct AsyncCatchupFailed(());

impl fmt::Display for AsyncCatchupFailed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Async RocksDB cache catchup failed or was canceled")
    }
}

impl error::Error for AsyncCatchupFailed {}

/// `OnceCell` equivalent that can be `.await`ed. Correspondingly, it has the following invariants:
///
/// - The cell is only set once
/// - The cell is always set to `Some(_)`.
///
/// `OnceCell` (either from `once_cell` or `tokio`) is not used because it lacks a way to wait for the cell
/// to be initialized. `once_cell::sync::OnceCell` has a blocking `wait()` method, but since it's blocking,
/// it risks spawning non-cancellable threads if misused.
type AsyncOnceCell<T> = watch::Receiver<Option<T>>;

/// A lazily initialized handle to RocksDB cache returned from [`AsyncCatchupTask::new()`].
#[derive(Debug)]
pub struct RocksdbCell {
    initial_state: AsyncOnceCell<InitialRocksdbState>,
    db: AsyncOnceCell<RocksdbStorage>,
}

impl RocksdbCell {
    /// Waits until RocksDB is initialized and returns it.
    ///
    /// # Errors
    ///
    /// Returns an error if the async catch-up task failed or was canceled before initialization.
    #[allow(clippy::missing_panics_doc)] // false positive
    pub async fn wait(&self) -> Result<RocksdbStorage, AsyncCatchupFailed> {
        self.db
            .clone()
            .wait_for(Option::is_some)
            .await
            // `unwrap` below is safe by construction
            .map(|db| db.clone().unwrap())
            .map_err(|_| AsyncCatchupFailed(()))
    }

    /// Gets a RocksDB instance if it has been initialized.
    pub fn get(&self) -> Option<RocksdbStorage> {
        self.db.borrow().clone()
    }

    /// Ensures that the RocksDB has started catching up, and returns the **initial** RocksDB state
    /// at the start of the catch-up.
    ///
    /// # Errors
    ///
    /// Returns an error if the async catch-up task failed or was canceled.
    #[allow(clippy::missing_panics_doc)] // false positive
    pub async fn ensure_initialized(&self) -> Result<InitialRocksdbState, AsyncCatchupFailed> {
        self.initial_state
            .clone()
            .wait_for(Option::is_some)
            .await
            // `unwrap` below is safe by construction
            .map(|state| state.clone().unwrap())
            .map_err(|_| AsyncCatchupFailed(()))
    }

    /// Creates a task that will keep storage updated after it has caught up.
    pub fn keep_updated(&self, pool: ConnectionPool<Core>) -> KeepUpdatedTask {
        KeepUpdatedTask {
            db: self.db.clone(),
            pool,
        }
    }
}

/// A runnable task that blocks until the provided RocksDB cache instance is caught up with
/// Postgres.
///
/// See [`ReadStorageFactory`] for more context.
#[derive(Debug)]
pub struct AsyncCatchupTask {
    pool: ConnectionPool<Core>,
    recovery_pool: ConnectionPool<Core>,
    state_keeper_db_path: String,
    state_keeper_db_options: RocksdbStorageOptions,
    initial_state_sender: watch::Sender<Option<InitialRocksdbState>>,
    db_sender: watch::Sender<Option<RocksdbStorage>>,
    to_l1_batch_number: Option<L1BatchNumber>,
}

impl AsyncCatchupTask {
    /// Create a new catch-up task with the provided Postgres and RocksDB instances. Optionally
    /// accepts the last L1 batch number to catch up to (defaults to latest if not specified).
    pub fn new(pool: ConnectionPool<Core>, state_keeper_db_path: String) -> (Self, RocksdbCell) {
        let (initial_state_sender, initial_state) = watch::channel(None);
        let (db_sender, db) = watch::channel(None);
        let this = Self {
            recovery_pool: pool.clone(),
            pool,
            state_keeper_db_path,
            state_keeper_db_options: RocksdbStorageOptions::default(),
            initial_state_sender,
            db_sender,
            to_l1_batch_number: None,
        };
        (this, RocksdbCell { initial_state, db })
    }

    /// Sets RocksDB options.
    #[must_use]
    pub fn with_db_options(mut self, options: RocksdbStorageOptions) -> Self {
        self.state_keeper_db_options = options;
        self
    }

    /// Sets the L1 batch number to catch up. By default, the task will catch up to the latest L1 batch
    /// (at the start of catch-up).
    #[must_use]
    pub fn with_target_l1_batch_number(mut self, number: L1BatchNumber) -> Self {
        self.to_l1_batch_number = Some(number);
        self
    }

    /// Sets up a separate DB connection pool for recovery.
    #[must_use]
    pub fn with_recovery_pool(mut self, pool: ConnectionPool<Core>) -> Self {
        self.recovery_pool = pool;
        self
    }

    /// Block until RocksDB cache instance is caught up with Postgres.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    #[tracing::instrument(name = "catch_up", skip_all, fields(target_l1_batch = ?self.to_l1_batch_number))]
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let started_at = Instant::now();
        tracing::info!("Catching up RocksDB asynchronously");

        let rocksdb_builder = RocksdbStorage::builder_with_options(
            self.state_keeper_db_path.as_ref(),
            self.state_keeper_db_options,
        )
        .await
        .context("Failed creating RocksDB storage builder")?;

        let initial_state = InitialRocksdbState {
            next_l1_batch_number: rocksdb_builder.next_l1_batch_number().await,
        };
        tracing::info!("Initialized RocksDB catchup from state: {initial_state:?}");
        self.initial_state_sender.send_replace(Some(initial_state));

        let Some((storage, init_strategy)) = rocksdb_builder
            .ensure_ready(&self.recovery_pool, &stop_receiver)
            .await
            .context("failed initializing state keeper RocksDB from snapshot or scratch")?
        else {
            tracing::info!("RocksDB cache recovery was interrupted");
            return Ok(());
        };

        if matches!(init_strategy, InitStrategy::Recovery) {
            let elapsed = started_at.elapsed();
            APP_METRICS.snapshot_recovery_latency[&SnapshotRecoveryStage::StateKeeperCache]
                .set(elapsed);
            tracing::info!("Recovered state keeper RocksDB from snapshot in {elapsed:?}");
        }

        let mut connection = self.pool.connection_tagged("state_keeper").await?;
        let rocksdb = storage
            .synchronize(&mut connection, &stop_receiver, self.to_l1_batch_number)
            .await
            .context("Failed to catch up RocksDB to Postgres")?;
        drop(connection);

        if let Some(rocksdb) = rocksdb {
            self.db_sender.send_replace(Some(rocksdb));
        } else {
            tracing::info!("Synchronizing RocksDB interrupted");
        }

        Ok(())
    }
}

#[derive(Debug)]
pub struct KeepUpdatedTask {
    db: AsyncOnceCell<RocksdbStorage>,
    pool: ConnectionPool<Core>,
}

impl KeepUpdatedTask {
    pub async fn run(mut self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        const SLEEP_INTERVAL: Duration = Duration::from_millis(100);

        let mut storage: RocksdbStorage = tokio::select! {
            res = self.db.wait_for(Option::is_some) => if let Ok(db) = res {
                // `unwrap()` is safe by construction
                db.clone().unwrap()
            } else {
                tracing::info!("Stop signal received, shutting down rocksdb updater task");
                return Ok(());
            },
            _ = stop_receiver.changed() => {
                tracing::info!("Stop signal received, shutting down rocksdb updater task");
                return Ok(());
            }
        };

        while !*stop_receiver.borrow() {
            let mut connection = self.pool.connection_tagged("rocksdb_updater_task").await?;
            storage = match storage
                .synchronize(&mut connection, &stop_receiver, None)
                .await
            {
                Ok(Some(storage)) => storage,
                Ok(None) => return Ok(()), // synchronization was interrupted
                Err(err) => {
                    return Err(err).context("Failed to catch up RocksDB to Postgres");
                }
            };
            drop(connection);

            tokio::time::sleep(SLEEP_INTERVAL).await;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use test_casing::test_casing;
    use zksync_types::L2BlockNumber;

    use super::*;
    use crate::test_utils::{create_l1_batch, create_l2_block, gen_storage_logs, prepare_postgres};

    #[tokio::test]
    async fn catching_up_basics() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        prepare_postgres(&mut conn).await;
        let storage_logs = gen_storage_logs(20..40);
        create_l2_block(&mut conn, L2BlockNumber(1), &storage_logs).await;
        create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;
        drop(conn);

        let temp_dir = TempDir::new().unwrap();
        let (task, rocksdb_cell) =
            AsyncCatchupTask::new(pool.clone(), temp_dir.path().to_str().unwrap().to_owned());
        let (_stop_sender, stop_receiver) = watch::channel(false);
        let task_handle = tokio::spawn(task.run(stop_receiver));

        let initial_state = rocksdb_cell.ensure_initialized().await.unwrap();
        assert_eq!(initial_state.next_l1_batch_number, None);

        let db = rocksdb_cell.wait().await.unwrap();
        assert_eq!(db.next_l1_batch_number().await, L1BatchNumber(2));
        task_handle.await.unwrap().unwrap();
        drop(db);
        drop(rocksdb_cell); // should be enough to release RocksDB lock

        let (task, rocksdb_cell) =
            AsyncCatchupTask::new(pool, temp_dir.path().to_str().unwrap().to_owned());
        let (_stop_sender, stop_receiver) = watch::channel(false);
        let task_handle = tokio::spawn(task.run(stop_receiver));

        let initial_state = rocksdb_cell.ensure_initialized().await.unwrap();
        assert_eq!(initial_state.next_l1_batch_number, Some(L1BatchNumber(2)));

        task_handle.await.unwrap().unwrap();
        rocksdb_cell.get().unwrap(); // RocksDB must be caught up at this point
    }

    #[tokio::test]
    async fn keep_updated_basics() {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        prepare_postgres(&mut conn).await;
        let storage_logs = gen_storage_logs(20..40);
        create_l2_block(&mut conn, L2BlockNumber(1), &storage_logs).await;
        create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;
        drop(conn);

        let temp_dir = TempDir::new().unwrap();
        let (catchup_task, rocksdb_cell) =
            AsyncCatchupTask::new(pool.clone(), temp_dir.path().to_str().unwrap().to_owned());
        let keep_updated_task = rocksdb_cell.keep_updated(pool.clone());
        let (_stop_sender, stop_receiver) = watch::channel(false);
        let catchup_task_handle = tokio::spawn(catchup_task.run(stop_receiver.clone()));
        let keep_updated_task_handle = tokio::spawn(keep_updated_task.run(stop_receiver));
        catchup_task_handle.await.unwrap().unwrap();

        let storage_logs = gen_storage_logs(40..50);
        let mut conn = pool.connection().await.unwrap();
        // Batch info should be inserted atomically.
        let mut transaction = conn.start_transaction().await.unwrap();
        create_l2_block(&mut transaction, L2BlockNumber(2), &storage_logs).await;
        create_l1_batch(&mut transaction, L1BatchNumber(2), &storage_logs).await;
        transaction.commit().await.unwrap();
        drop(conn);

        let rocksdb = rocksdb_cell.get().unwrap();
        let started_at = Instant::now();
        loop {
            if started_at.elapsed() > Duration::from_secs(10) {
                break;
            }
            if rocksdb.next_l1_batch_number().await == L1BatchNumber(3) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        if keep_updated_task_handle.is_finished() {
            panic!(
                "KeepUpdated task finished: {:?}",
                keep_updated_task_handle.await
            );
        } else {
            panic!("Timeout waiting for catch up");
        }
    }

    #[derive(Debug)]
    enum CancellationScenario {
        DropTask,
        CancelTask,
    }

    impl CancellationScenario {
        const ALL: [Self; 2] = [Self::DropTask, Self::CancelTask];
    }

    #[test_casing(2, CancellationScenario::ALL)]
    #[tokio::test]
    async fn catching_up_cancellation(scenario: CancellationScenario) {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        prepare_postgres(&mut conn).await;
        let storage_logs = gen_storage_logs(20..40);
        create_l2_block(&mut conn, L2BlockNumber(1), &storage_logs).await;
        create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;
        drop(conn);

        let temp_dir = TempDir::new().unwrap();
        let (task, rocksdb_cell) =
            AsyncCatchupTask::new(pool.clone(), temp_dir.path().to_str().unwrap().to_owned());
        let (stop_sender, stop_receiver) = watch::channel(false);
        match scenario {
            CancellationScenario::DropTask => drop(task),
            CancellationScenario::CancelTask => {
                stop_sender.send_replace(true);
                task.run(stop_receiver).await.unwrap();
            }
        }

        assert!(rocksdb_cell.get().is_none());
        rocksdb_cell.wait().await.unwrap_err();
    }
}
