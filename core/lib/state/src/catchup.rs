use std::{
    error, fmt,
    time::{Duration, Instant},
};

use anyhow::Context;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_shared_metrics::{SnapshotRecoveryStage, APP_METRICS};
use zksync_storage::RocksDB;
use zksync_types::L1BatchNumber;

use crate::{
    RocksdbStorage, RocksdbStorageBuilder, RocksdbStorageOptions, StateKeeperColumnFamily,
};

/// Initial RocksDB cache state returned by [`RocksdbCell::ensure_initialized()`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InitialRocksdbState {
    /// Last processed L1 batch number in the RocksDB cache + 1 (i.e., the batch that the cache is ready to process).
    /// `None` if the cache is empty (i.e., needs recovery).
    pub l1_batch_number: Option<L1BatchNumber>,
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
    db: AsyncOnceCell<RocksDB<StateKeeperColumnFamily>>,
}

impl RocksdbCell {
    /// Waits until RocksDB is initialized and returns it.
    ///
    /// # Errors
    ///
    /// Returns an error if the async catch-up task failed or was canceled before initialization.
    #[allow(clippy::missing_panics_doc)] // false positive
    pub async fn wait(&self) -> Result<RocksDB<StateKeeperColumnFamily>, AsyncCatchupFailed> {
        self.db
            .clone()
            .wait_for(Option::is_some)
            .await
            // `unwrap` below is safe by construction
            .map(|db| db.clone().unwrap())
            .map_err(|_| AsyncCatchupFailed(()))
    }

    /// Gets a RocksDB instance if it has been initialized.
    pub fn get(&self) -> Option<RocksDB<StateKeeperColumnFamily>> {
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
}

/// A runnable task that blocks until the provided RocksDB cache instance is caught up with
/// Postgres.
///
/// See [`ReadStorageFactory`] for more context.
#[derive(Debug)]
pub struct AsyncCatchupTask {
    pool: ConnectionPool<Core>,
    state_keeper_db_path: String,
    state_keeper_db_options: RocksdbStorageOptions,
    initial_state_sender: watch::Sender<Option<InitialRocksdbState>>,
    db_sender: watch::Sender<Option<RocksDB<StateKeeperColumnFamily>>>,
    to_l1_batch_number: Option<L1BatchNumber>,
    catchup_indefinitely: bool,
}

impl AsyncCatchupTask {
    /// Create a new catch-up task with the provided Postgres and RocksDB instances. Optionally
    /// accepts the last L1 batch number to catch up to (defaults to latest if not specified).
    pub fn new(
        pool: ConnectionPool<Core>,
        state_keeper_db_path: String,
        catchup_indefinitely: bool,
    ) -> (Self, RocksdbCell) {
        let (initial_state_sender, initial_state) = watch::channel(None);
        let (db_sender, db) = watch::channel(None);
        let this = Self {
            pool,
            state_keeper_db_path,
            state_keeper_db_options: RocksdbStorageOptions::default(),
            initial_state_sender,
            db_sender,
            to_l1_batch_number: None,
            catchup_indefinitely,
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

    /// Block until RocksDB cache instance is caught up with Postgres.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    #[tracing::instrument(name = "catch_up", skip_all, fields(target_l1_batch = ?self.to_l1_batch_number))]
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let started_at = Instant::now();
        tracing::info!("Catching up RocksDB asynchronously");

        let mut rocksdb_builder = RocksdbStorage::builder_with_options(
            self.state_keeper_db_path.as_ref(),
            self.state_keeper_db_options,
        )
        .await
        .context("Failed creating RocksDB storage builder")?;

        let initial_state = InitialRocksdbState {
            l1_batch_number: rocksdb_builder.l1_batch_number().await,
        };
        tracing::info!("Initialized RocksDB catchup from state: {initial_state:?}");
        self.initial_state_sender.send_replace(Some(initial_state));

        let mut connection = self.pool.connection_tagged("state_keeper").await?;
        let was_recovered_from_snapshot = rocksdb_builder
            .ensure_ready(&mut connection, &stop_receiver)
            .await
            .context("failed initializing state keeper RocksDB from snapshot or scratch")?;
        if was_recovered_from_snapshot {
            let elapsed = started_at.elapsed();
            APP_METRICS.snapshot_recovery_latency[&SnapshotRecoveryStage::StateKeeperCache]
                .set(elapsed);
            tracing::info!("Recovered state keeper RocksDB from snapshot in {elapsed:?}");
        }

        let rocksdb = rocksdb_builder
            .synchronize(&mut connection, &stop_receiver, self.to_l1_batch_number)
            .await
            .context("Failed to catch up RocksDB to Postgres")?;
        drop(connection);

        let rocksdb = rocksdb.map(RocksdbStorage::into_rocksdb);
        if let Some(rocksdb) = &rocksdb {
            self.db_sender.send_replace(Some(rocksdb.clone()));
        } else {
            tracing::info!("Synchronizing RocksDB interrupted");
        }

        if self.catchup_indefinitely {
            if let Some(rocksdb) = rocksdb {
                self.update_loop(stop_receiver, rocksdb).await?;
            }
        }

        Ok(())
    }

    async fn update_loop(
        self,
        stop_receiver: watch::Receiver<bool>,
        rocksdb: RocksDB<StateKeeperColumnFamily>,
    ) -> anyhow::Result<()> {
        const SLEEP_INTERVAL: Duration = Duration::from_millis(10);

        let mut rocksdb_builder = RocksdbStorageBuilder::from_rocksdb(rocksdb);
        let mut connection = self.pool.connection_tagged("state_keeper").await?;

        loop {
            if *stop_receiver.borrow() {
                return Ok(());
            }
            rocksdb_builder
                .update_from_postgres(&mut connection, &stop_receiver, None)
                .await
                .context("Failed to catch up RocksDB to Postgres")?;

            tokio::time::sleep(SLEEP_INTERVAL).await;
        }
    }

    /// Returns whether task in oneshot, i.e. whether it's supposed to stop after some time.
    pub fn is_oneshot(&self) -> bool {
        !self.catchup_indefinitely
    }
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;
    use test_casing::test_casing;
    use zksync_types::L2BlockNumber;

    use super::*;
    use crate::{
        test_utils::{create_l1_batch, create_l2_block, gen_storage_logs, prepare_postgres},
        RocksdbStorageBuilder,
    };

    #[test_casing(2, [false, true])]
    #[tokio::test]
    async fn catching_up_basics(catchup_indefinitely: bool) {
        let pool = ConnectionPool::<Core>::test_pool().await;
        let mut conn = pool.connection().await.unwrap();
        prepare_postgres(&mut conn).await;
        let storage_logs = gen_storage_logs(20..40);
        create_l2_block(&mut conn, L2BlockNumber(1), storage_logs.clone()).await;
        create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;
        drop(conn);

        let temp_dir = TempDir::new().unwrap();
        let (task, rocksdb_cell) = AsyncCatchupTask::new(
            pool.clone(),
            temp_dir.path().to_str().unwrap().to_owned(),
            catchup_indefinitely,
        );
        let (_stop_sender, stop_receiver) = watch::channel(false);
        let task_handle = tokio::spawn(task.run(stop_receiver));

        let initial_state = rocksdb_cell.ensure_initialized().await.unwrap();
        assert_eq!(initial_state.l1_batch_number, None);

        loop {
            if let Some(db) = rocksdb_cell.get() {
                let l1_batch_number = RocksdbStorageBuilder::from_rocksdb(db)
                    .l1_batch_number()
                    .await;
                if l1_batch_number == Some(L1BatchNumber(2)) {
                    break;
                }
            } else {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        let rocksdb_cell = if !catchup_indefinitely {
            task_handle.await.unwrap().unwrap();
            drop(rocksdb_cell); // should be enough to release RocksDB lock

            let (task, rocksdb_cell) = AsyncCatchupTask::new(
                pool.clone(),
                temp_dir.path().to_str().unwrap().to_owned(),
                catchup_indefinitely,
            );
            let (_stop_sender, stop_receiver) = watch::channel(false);
            let task_handle = tokio::spawn(task.run(stop_receiver));

            let initial_state = rocksdb_cell.ensure_initialized().await.unwrap();
            assert_eq!(initial_state.l1_batch_number, Some(L1BatchNumber(2)));

            task_handle.await.unwrap().unwrap();

            rocksdb_cell
        } else {
            rocksdb_cell
        };

        // Check that DB continues catching up.
        if catchup_indefinitely {
            let storage_logs = gen_storage_logs(40..50);
            let mut conn = pool.connection().await.unwrap();
            create_l2_block(&mut conn, L2BlockNumber(2), storage_logs.clone()).await;
            create_l1_batch(&mut conn, L1BatchNumber(2), &storage_logs).await;
            drop(conn);

            let rocksdb = rocksdb_cell.get().unwrap();
            let builder = RocksdbStorageBuilder::from_rocksdb(rocksdb);
            loop {
                if builder.l1_batch_number().await == Some(L1BatchNumber(3)) {
                    break;
                } else {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
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
        create_l2_block(&mut conn, L2BlockNumber(1), storage_logs.clone()).await;
        create_l1_batch(&mut conn, L1BatchNumber(1), &storage_logs).await;
        drop(conn);

        let temp_dir = TempDir::new().unwrap();
        let (task, rocksdb_cell) = AsyncCatchupTask::new(
            pool.clone(),
            temp_dir.path().to_str().unwrap().to_owned(),
            false,
        );
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
