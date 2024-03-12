use std::{fmt::Debug, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use tokio::{runtime::Handle, sync::watch};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_state::{
    open_state_keeper_rocksdb, PostgresStorage, ReadStorage, RocksdbStorage, RocksdbStorageBuilder,
    StateKeeperColumnFamily,
};
use zksync_storage::RocksDB;
use zksync_types::{L1BatchNumber, MiniblockNumber};

/// Factory that can produce a [`ReadStorage`] implementation on demand.
#[async_trait]
pub trait ReadStorageFactory: Debug + Send + Sync + 'static {
    async fn access_storage<'a>(
        &'a self,
        rt_handle: Handle,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'a>>>;
}

/// A [`ReadStorage`] implementation that uses either [`PostgresStorage`] or [`RocksdbStorage`]
/// underneath.
#[derive(Debug)]
pub enum PgOrRocksdbStorage<'a> {
    Postgres(PostgresStorage<'a>),
    Rocksdb(RocksdbStorage),
}

impl ReadStorage for PgOrRocksdbStorage<'_> {
    fn read_value(&mut self, key: &zksync_types::StorageKey) -> zksync_types::StorageValue {
        match self {
            Self::Postgres(postgres) => postgres.read_value(key),
            Self::Rocksdb(rocksdb) => rocksdb.read_value(key),
        }
    }

    fn is_write_initial(&mut self, key: &zksync_types::StorageKey) -> bool {
        match self {
            Self::Postgres(postgres) => postgres.is_write_initial(key),
            Self::Rocksdb(rocksdb) => rocksdb.is_write_initial(key),
        }
    }

    fn load_factory_dep(&mut self, hash: zksync_types::H256) -> Option<Vec<u8>> {
        match self {
            Self::Postgres(postgres) => postgres.load_factory_dep(hash),
            Self::Rocksdb(rocksdb) => rocksdb.load_factory_dep(hash),
        }
    }

    fn get_enumeration_index(&mut self, key: &zksync_types::StorageKey) -> Option<u64> {
        match self {
            Self::Postgres(postgres) => postgres.get_enumeration_index(key),
            Self::Rocksdb(rocksdb) => rocksdb.get_enumeration_index(key),
        }
    }
}

impl<'a> From<PostgresStorage<'a>> for PgOrRocksdbStorage<'a> {
    fn from(value: PostgresStorage<'a>) -> Self {
        Self::Postgres(value)
    }
}

impl<'a> From<RocksdbStorage> for PgOrRocksdbStorage<'a> {
    fn from(value: RocksdbStorage) -> Self {
        Self::Rocksdb(value)
    }
}

/// A [`ReadStorageFactory`] implementation that can produce short-lived [`ReadStorage`] handles
/// backed by either Postgres or RocksDB (if it's caught up). Always initialized as a `Postgres`
/// variant and is then mutated into `Rocksdb` once RocksDB cache is caught up. After which it
/// can never revert back to `Postgres` as we assume RocksDB cannot fall behind under normal state
/// keeper operation.
#[derive(Debug, Clone)]
pub struct AsyncRocksdbCache {
    pool: ConnectionPool,
    rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
}

impl AsyncRocksdbCache {
    /// Load latest sealed miniblock from the latest sealed L1 batch (ignores sealed miniblocks
    /// from unsealed batches).
    async fn load_latest_sealed_miniblock(
        connection: &mut StorageProcessor<'_>,
    ) -> anyhow::Result<Option<(MiniblockNumber, L1BatchNumber)>> {
        let mut dal = connection.blocks_dal();
        let Some(l1_batch_number) = dal.get_sealed_l1_batch_number().await? else {
            return Ok(None);
        };
        let Some((_, miniblock_number)) =
            dal.get_miniblock_range_of_l1_batch(l1_batch_number).await?
        else {
            return Ok(None);
        };
        Ok(Some((miniblock_number, l1_batch_number)))
    }

    /// Returns a [`ReadStorage`] implementation backed by Postgres
    async fn access_storage_pg(
        rt_handle: Handle,
        pool: &ConnectionPool,
    ) -> anyhow::Result<PgOrRocksdbStorage<'_>> {
        let mut connection = pool.access_storage().await?;

        let (miniblock_number, l1_batch_number) =
            match Self::load_latest_sealed_miniblock(&mut connection).await? {
                Some((miniblock_number, l1_batch_number)) => (miniblock_number, l1_batch_number),
                None => {
                    tracing::info!("Could not find latest sealed miniblock, loading from snapshot");
                    let snapshot_recovery = connection
                        .snapshot_recovery_dal()
                        .get_applied_snapshot_status()
                        .await
                        .context("failed getting snapshot recovery info")?
                        .ok_or_else(|| {
                            anyhow::anyhow!("Could not find snapshot, no state available")
                        })?;
                    (
                        snapshot_recovery.miniblock_number,
                        snapshot_recovery.l1_batch_number,
                    )
                }
            };

        tracing::debug!(%l1_batch_number, %miniblock_number, "Using Postgres-based storage");
        Ok(
            PostgresStorage::new_async(rt_handle, connection, miniblock_number, true)
                .await?
                .into(),
        )
    }

    /// Catches up RocksDB synchronously (i.e. assumes the gap is small) and
    /// returns a [`ReadStorage`] implementation backed by caught-up RocksDB.
    async fn access_storage_rocksdb<'a>(
        conn: &mut StorageProcessor<'_>,
        rocksdb: RocksDB<StateKeeperColumnFamily>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'a>>> {
        tracing::debug!("Catching up RocksDB synchronously");
        let rocksdb_builder: RocksdbStorageBuilder = rocksdb.into();
        let rocksdb = rocksdb_builder
            .synchronize(conn, stop_receiver)
            .await
            .context("Failed to catch up state keeper RocksDB storage to Postgres")?;
        let Some(rocksdb) = rocksdb else {
            tracing::info!("Synchronizing RocksDB interrupted");
            return Ok(None);
        };
        let rocksdb_l1_batch_number = rocksdb.l1_batch_number().await.unwrap_or_default();
        tracing::debug!(%rocksdb_l1_batch_number, "Using RocksDB-based storage");
        Ok(Some(rocksdb.into()))
    }

    async fn access_storage_inner<'a>(
        &'a self,
        rt_handle: Handle,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'a>>> {
        match self.rocksdb_cell.get() {
            Some(rocksdb) => {
                let mut conn = self
                    .pool
                    .access_storage_tagged("state_keeper")
                    .await
                    .context("Failed getting a Postgres connection")?;
                Self::access_storage_rocksdb(&mut conn, rocksdb.clone(), stop_receiver)
                    .await
                    .context("Failed accessing RocksDB storage")
            }
            None => Ok(Some(
                Self::access_storage_pg(rt_handle, &self.pool)
                    .await
                    .context("Failed accessing Postgres storage")?,
            )),
        }
    }

    pub fn new(
        pool: ConnectionPool,
        state_keeper_db_path: String,
        enum_index_migration_chunk_size: usize,
        stop_receiver: watch::Receiver<bool>,
    ) -> Self {
        let rocksdb_cell = Arc::new(OnceCell::new());
        let task = AsyncCatchupTask {
            pool: pool.clone(),
            state_keeper_db_path,
            enum_index_migration_chunk_size,
            rocksdb_cell: rocksdb_cell.clone(),
        };
        tokio::task::spawn(async move { task.run(stop_receiver).await.unwrap() });
        Self { pool, rocksdb_cell }
    }
}

#[async_trait]
impl ReadStorageFactory for AsyncRocksdbCache {
    async fn access_storage<'a>(
        &'a self,
        rt_handle: Handle,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'a>>> {
        self.access_storage_inner(rt_handle, stop_receiver).await
    }
}

struct AsyncCatchupTask {
    pool: ConnectionPool,
    state_keeper_db_path: String,
    enum_index_migration_chunk_size: usize,
    rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
}

impl AsyncCatchupTask {
    async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::debug!("Catching up RocksDB asynchronously");
        let mut rocksdb_builder: RocksdbStorageBuilder =
            open_state_keeper_rocksdb(self.state_keeper_db_path.into())
                .await
                .context("Failed initializing RocksDB storage")?
                .into();
        rocksdb_builder.enable_enum_index_migration(self.enum_index_migration_chunk_size);
        let mut storage = self
            .pool
            .access_storage()
            .await
            .context("Failed accessing Postgres storage")?;
        let rocksdb = rocksdb_builder
            .synchronize(&mut storage, &stop_receiver)
            .await
            .context("Failed to catch up RocksDB to Postgres")?;
        drop(storage);
        if let Some(rocksdb) = rocksdb {
            self.rocksdb_cell
                .set(rocksdb.into())
                .map_err(|_| anyhow::anyhow!("Async RocksDB cache was initialized twice"))?;
        } else {
            tracing::info!("Synchronizing RocksDB interrupted");
        }
        Ok(())
    }
}
