use std::sync::Arc;
use std::time::Instant;

use anyhow::Context;
use once_cell::sync::OnceCell;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_shared_metrics::{SnapshotRecoveryStage, APP_METRICS};
use zksync_storage::RocksDB;
use zksync_types::L1BatchNumber;

use crate::{RocksdbStorage, StateKeeperColumnFamily};

/// A runnable task that blocks until the provided RocksDB cache instance is caught up with
/// Postgres.
///
/// See [`ReadStorageFactory`] for more context.
#[derive(Debug)]
pub struct AsyncCatchupTask {
    pool: ConnectionPool<Core>,
    state_keeper_db_path: String,
    rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
    to_l1_batch_number: Option<L1BatchNumber>,
}

impl AsyncCatchupTask {
    /// Create a new catch-up task with the provided Postgres and RocksDB instances. Optionally
    /// accepts the last L1 batch number to catch up to (defaults to latest if not specified).
    pub fn new(
        pool: ConnectionPool<Core>,
        state_keeper_db_path: String,
        rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
        to_l1_batch_number: Option<L1BatchNumber>,
    ) -> Self {
        Self {
            pool,
            state_keeper_db_path,
            rocksdb_cell,
            to_l1_batch_number,
        }
    }

    /// Block until RocksDB cache instance is caught up with Postgres.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let started_at = Instant::now();
        tracing::debug!("Catching up RocksDB asynchronously");

        let mut rocksdb_builder = RocksdbStorage::builder(self.state_keeper_db_path.as_ref())
            .await
            .context("Failed creating RocksDB storage builder")?;
        let mut connection = self.pool.connection().await?;
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
            self.rocksdb_cell
                .set(rocksdb.into_rocksdb())
                .map_err(|_| anyhow::anyhow!("Async RocksDB cache was initialized twice"))?;
        } else {
            tracing::info!("Synchronizing RocksDB interrupted");
        }
        Ok(())
    }
}
