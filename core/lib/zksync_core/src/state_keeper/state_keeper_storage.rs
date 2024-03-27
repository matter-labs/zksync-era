use std::{fmt::Debug, sync::Arc};

use anyhow::Context;
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use tokio::{runtime::Handle, sync::watch};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_state::{
    PostgresStorage, ReadStorage, RocksdbStorage, RocksdbStorageBuilder, StateKeeperColumnFamily,
};
use zksync_storage::RocksDB;
use zksync_types::{L1BatchNumber, MiniblockNumber};

/// Factory that can produce a [`ReadStorage`] implementation on demand.
#[async_trait]
pub trait ReadStorageFactory: Debug + Send + Sync + 'static {
    async fn access_storage(
        &self,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>>;
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
    pool: ConnectionPool<Core>,
    rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
}

impl AsyncRocksdbCache {
    /// Load latest sealed miniblock from the latest sealed L1 batch (ignores sealed miniblocks
    /// from unsealed batches).
    async fn load_latest_sealed_miniblock(
        connection: &mut Connection<'_, Core>,
    ) -> anyhow::Result<Option<(MiniblockNumber, L1BatchNumber)>> {
        let mut dal = connection.blocks_dal();
        let Some(l1_batch_number) = dal
            .get_sealed_l1_batch_number()
            .await
            .context("Failed to load the latest sealed L1 batch number")?
        else {
            return Ok(None);
        };
        let (_, miniblock_number) = dal
            .get_miniblock_range_of_l1_batch(l1_batch_number)
            .await
            .context("Failed to load the miniblock range for the latest sealed L1 batch")?
            .context("The latest sealed L1 batch does not have a miniblock range")?;
        Ok(Some((miniblock_number, l1_batch_number)))
    }

    /// Returns a [`ReadStorage`] implementation backed by Postgres
    pub(crate) async fn access_storage_pg(
        pool: &ConnectionPool<Core>,
    ) -> anyhow::Result<PgOrRocksdbStorage<'_>> {
        let mut connection = pool.connection().await?;

        let (miniblock_number, l1_batch_number) =
            match Self::load_latest_sealed_miniblock(&mut connection).await? {
                Some((miniblock_number, l1_batch_number)) => (miniblock_number, l1_batch_number),
                None => {
                    tracing::info!("Could not find latest sealed miniblock, loading from snapshot");
                    let snapshot_recovery = connection
                        .snapshot_recovery_dal()
                        .get_applied_snapshot_status()
                        .await
                        .context("Failed getting snapshot recovery info")?
                        .context("Could not find snapshot, no state available")?;
                    (
                        snapshot_recovery.miniblock_number,
                        snapshot_recovery.l1_batch_number,
                    )
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
        tracing::debug!(%rocksdb_l1_batch_number, "Using RocksDB-based storage");
        Ok(Some(rocksdb.into()))
    }

    async fn access_storage_inner(
        &self,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        if let Some(rocksdb) = self.rocksdb_cell.get() {
            let mut connection = self
                .pool
                .connection_tagged("state_keeper")
                .await
                .context("Failed getting a Postgres connection")?;
            Self::access_storage_rocksdb(&mut connection, rocksdb.clone(), stop_receiver)
                .await
                .context("Failed accessing RocksDB storage")
        } else {
            Ok(Some(
                Self::access_storage_pg(&self.pool)
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
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        self.access_storage_inner(stop_receiver).await
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
