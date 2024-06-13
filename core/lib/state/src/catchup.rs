use std::{error, fmt, time::Instant};

use anyhow::Context;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_shared_metrics::{SnapshotRecoveryStage, APP_METRICS};
use zksync_storage::RocksDB;
use zksync_types::L1BatchNumber;

use crate::{RocksdbStorage, RocksdbStorageOptions, StateKeeperColumnFamily};

/// Initial RocksDB cache state returned by [`RocksdbCell::ensure_initialized()`].
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct InitialRocksdbState {
    /// Initial L1 batch number in the RocksDB cache. `None` if the cache is empty (i.e., needs recovery).
    pub l1_batch_number: Option<L1BatchNumber>,
}

/// Error returned from [`RocksdbCell`] methods if the corresponding [`AsyncCatchupTask`] has failed.
#[derive(Debug)]
pub struct AsyncCatchupFailed(());

impl fmt::Display for AsyncCatchupFailed {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Async RocksDB cache catchup failed")
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
    /// Returns an error if the async catch-up task failed before initialization.
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
    /// Returns an error if the async catch-up task failed.
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
}

impl AsyncCatchupTask {
    /// Create a new catch-up task with the provided Postgres and RocksDB instances. Optionally
    /// accepts the last L1 batch number to catch up to (defaults to latest if not specified).
    pub fn new(
        pool: ConnectionPool<Core>,
        state_keeper_db_path: String,
        state_keeper_db_options: RocksdbStorageOptions,
        to_l1_batch_number: Option<L1BatchNumber>,
    ) -> (Self, RocksdbCell) {
        let (initial_state_sender, initial_state) = watch::channel(None);
        let (db_sender, db) = watch::channel(None);
        let this = Self {
            pool,
            state_keeper_db_path,
            state_keeper_db_options,
            initial_state_sender,
            db_sender,
            to_l1_batch_number,
        };
        (this, RocksdbCell { initial_state, db })
    }

    /// Block until RocksDB cache instance is caught up with Postgres.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let started_at = Instant::now();
        tracing::debug!("Catching up RocksDB asynchronously");

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
        if let Some(rocksdb) = rocksdb {
            self.db_sender.send_replace(Some(rocksdb.into_rocksdb()));
        } else {
            tracing::info!("Synchronizing RocksDB interrupted");
        }
        Ok(())
    }
}
