use std::sync::Arc;

use anyhow::Context;
use once_cell::sync::OnceCell;
use tokio::sync::watch;
use zksync_dal::{ConnectionPool, Core};
use zksync_storage::RocksDB;
use zksync_types::L1BatchNumber;

use crate::{RocksdbStorage, RocksdbStorageBuilder, StateKeeperColumnFamily};

/// A runnable task that blocks until the provided RocksDB cache instance is caught up with
/// Postgres.
///
/// See [`ReadStorageFactory`] for more context.
#[derive(Debug)]
pub struct AsyncCatchupTask {
    pool: ConnectionPool<Core>,
    state_keeper_db_path: String,
    enum_index_migration_chunk_size: usize,
    rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
    to_l1_batch_number: Option<L1BatchNumber>,
}

impl AsyncCatchupTask {
    /// Create a new catch-up task with the provided Postgres and RocksDB instances. Optionally
    /// accepts the last L1 batch number to catch up to (defaults to latest if not specified).
    pub fn new(
        pool: ConnectionPool<Core>,
        state_keeper_db_path: String,
        enum_index_migration_chunk_size: usize,
        rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
        to_l1_batch_number: Option<L1BatchNumber>,
    ) -> Self {
        Self {
            pool,
            state_keeper_db_path,
            enum_index_migration_chunk_size,
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
        tracing::debug!("Catching up RocksDB asynchronously");
        let mut rocksdb_builder: RocksdbStorageBuilder =
            RocksdbStorage::builder(self.state_keeper_db_path.as_ref())
                .await
                .context("Failed initializing RocksDB storage")?;
        rocksdb_builder.enable_enum_index_migration(self.enum_index_migration_chunk_size);
        let mut connection = self
            .pool
            .connection()
            .await
            .context("Failed accessing Postgres storage")?;
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
