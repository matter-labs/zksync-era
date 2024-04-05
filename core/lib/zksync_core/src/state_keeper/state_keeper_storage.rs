use std::{fmt::Debug, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use tokio::{runtime::Handle, sync::watch};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_state::{
    PgOrRocksdbStorage, PostgresStorage, ReadStorageFactory, RocksdbStorage, RocksdbStorageBuilder,
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
    /// Returns a [`ReadStorage`] implementation backed by Postgres
    pub(crate) async fn access_storage_pg(
        pool: &ConnectionPool<Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<PgOrRocksdbStorage<'_>> {
        let mut connection = pool.connection().await?;
        let mut dal = connection.blocks_dal();
        let miniblock_number = match dal
            .get_miniblock_range_of_l1_batch(l1_batch_number)
            .await
            .context("Failed to load the miniblock range for the latest sealed L1 batch")?
        {
            Some((_, miniblock_number)) => miniblock_number,
            None => {
                tracing::info!("Could not find latest sealed miniblock, loading from snapshot");
                let snapshot_recovery = connection
                    .snapshot_recovery_dal()
                    .get_applied_snapshot_status()
                    .await
                    .context("Failed getting snapshot recovery info")?
                    .context("Could not find snapshot, no state available")?;
                if snapshot_recovery.l1_batch_number != l1_batch_number {
                    anyhow::bail!(
                        "Snapshot contains L1 batch #{} while #{} was expected",
                        snapshot_recovery.l1_batch_number,
                        l1_batch_number
                    );
                }
                snapshot_recovery.miniblock_number
            }
        };
        tracing::debug!(%l1_batch_number, %miniblock_number, "Using Postgres-based storage");
        Ok(
            PostgresStorage::new_async(Handle::current(), connection, miniblock_number, true)
                .await?
                .into(),
        )
    }

    /// Catches up RocksDB synchronously (i.e. assumes the gap is small) and
    /// returns a [`ReadStorage`] implementation backed by caught-up RocksDB.
    async fn access_storage_rocksdb<'a>(
        connection: &mut Connection<'_, Core>,
        rocksdb: RocksDB<StateKeeperColumnFamily>,
        stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'a>>> {
        tracing::debug!("Catching up RocksDB synchronously");
        let rocksdb_builder = RocksdbStorageBuilder::from_rocksdb(rocksdb);
        let rocksdb = rocksdb_builder
            .synchronize(connection, stop_receiver)
            .await
            .context("Failed to catch up state keeper RocksDB storage to Postgres")?;
        let Some(rocksdb) = rocksdb else {
            tracing::info!("Synchronizing RocksDB interrupted");
            return Ok(None);
        };
        let rocksdb_l1_batch_number = rocksdb.l1_batch_number().await.unwrap_or_default();
        if l1_batch_number != rocksdb_l1_batch_number {
            anyhow::bail!(
                "RocksDB synchronized to L1 batch #{} while #{} was expected",
                rocksdb_l1_batch_number,
                l1_batch_number
            );
        }
        tracing::debug!(%rocksdb_l1_batch_number, "Using RocksDB-based storage");
        Ok(Some(rocksdb.into()))
    }

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
            Self::access_storage_rocksdb(
                &mut connection,
                rocksdb.clone(),
                stop_receiver,
                l1_batch_number,
            )
            .await
            .context("Failed accessing RocksDB storage")
        } else {
            Ok(Some(
                Self::access_storage_pg(&self.pool, l1_batch_number)
                    .await
                    .context("Failed accessing Postgres storage")?,
            ))
        }
    }

    pub fn new(
        pool: ConnectionPool<Core>,
        state_keeper_db_path: String,
        enum_index_migration_chunk_size: usize,
    ) -> (Self, AsyncCatchupTask) {
        let rocksdb_cell = Arc::new(OnceCell::new());
        let task = AsyncCatchupTask {
            pool: pool.clone(),
            state_keeper_db_path,
            enum_index_migration_chunk_size,
            rocksdb_cell: rocksdb_cell.clone(),
        };
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

#[derive(Debug)]
pub struct AsyncCatchupTask {
    pool: ConnectionPool<Core>,
    state_keeper_db_path: String,
    enum_index_migration_chunk_size: usize,
    rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
}

impl AsyncCatchupTask {
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
            .synchronize(&mut connection, &stop_receiver)
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
