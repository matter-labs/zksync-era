use std::{
    fmt::Debug,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use async_trait::async_trait;
use tokio::{runtime::Handle, sync::watch};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_state::{PostgresStorage, ReadStorage, RocksdbStorage, StateKeeperColumnFamily};
use zksync_storage::RocksDB;

/// Encapsulates a storage that can produce a [`ReadStorageFactory`] on demand.
///
/// This struct's main design purpose is to be able to produce a (cheap) owned [`ReadStorageFactory`]
/// implementation that can then be used to obtain a short-lived [`ReadStorage`] handle bound to
/// the respective factory.
#[derive(Debug, Clone)]
pub struct StateKeeperStorage<T: ReadStorageFactory> {
    // FIXME: `Arc<Mutex<_>>` is a bit of a leaky abstraction here as it exists mostly because of
    // the mutable nature of [`AsyncRocksdbCache`].
    inner: Arc<Mutex<T>>,
}

/// Factory that can produce a [`ReadStorage`] implementation on demand.
#[async_trait]
pub trait ReadStorageFactory: Clone + Debug + Send + Sync {
    type ReadStorageImpl<'a>: ReadStorage
    where
        Self: 'a;

    async fn access_storage<'a>(
        &'a self,
        rt_handle: Handle,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Option<Self::ReadStorageImpl<'a>>;
}

/// A [`ReadStorage`] implementation that uses either [`PostgresStorage`] or [`RocksdbStorage`]
/// underneath.
#[derive(Debug)]
pub enum PgOrRocksdbStorage<'a> {
    Postgres(PostgresStorage<'a>),
    RocksdbStorage(RocksdbStorage),
}

impl<'a> ReadStorage for PgOrRocksdbStorage<'a> {
    fn read_value(&mut self, key: &zksync_types::StorageKey) -> zksync_types::StorageValue {
        match self {
            PgOrRocksdbStorage::Postgres(postgres) => postgres.read_value(key),
            PgOrRocksdbStorage::RocksdbStorage(rocksdb) => rocksdb.read_value(key),
        }
    }

    fn is_write_initial(&mut self, key: &zksync_types::StorageKey) -> bool {
        match self {
            PgOrRocksdbStorage::Postgres(postgres) => postgres.is_write_initial(key),
            PgOrRocksdbStorage::RocksdbStorage(rocksdb) => rocksdb.is_write_initial(key),
        }
    }

    fn load_factory_dep(&mut self, hash: zksync_types::H256) -> Option<Vec<u8>> {
        match self {
            PgOrRocksdbStorage::Postgres(postgres) => postgres.load_factory_dep(hash),
            PgOrRocksdbStorage::RocksdbStorage(rocksdb) => rocksdb.load_factory_dep(hash),
        }
    }

    fn get_enumeration_index(&mut self, key: &zksync_types::StorageKey) -> Option<u64> {
        match self {
            PgOrRocksdbStorage::Postgres(postgres) => postgres.get_enumeration_index(key),
            PgOrRocksdbStorage::RocksdbStorage(rocksdb) => rocksdb.get_enumeration_index(key),
        }
    }
}

impl<'a> From<PostgresStorage<'a>> for PgOrRocksdbStorage<'a> {
    fn from(value: PostgresStorage<'a>) -> Self {
        PgOrRocksdbStorage::Postgres(value)
    }
}

impl<'a> From<RocksdbStorage> for PgOrRocksdbStorage<'a> {
    fn from(value: RocksdbStorage) -> Self {
        PgOrRocksdbStorage::RocksdbStorage(value)
    }
}

/// A [`ReadStorageFactory`] implementation that can produce short-lived [`ReadStorage`] handles
/// backed by either Postgres or RocksDB (if it's caught up). Always initialized as a `Postgres`
/// variant and is then mutated into `Rocksdb` once RocksDB cache is caught up. After which it
/// can never revert back to `Postgres` as we assume RocksDB cannot fall behind under normal state
/// keeper operation.
#[derive(Debug, Clone)]
pub enum AsyncRocksdbCache {
    Postgres(ConnectionPool),
    Rocksdb(RocksDB<StateKeeperColumnFamily>, ConnectionPool),
}

impl AsyncRocksdbCache {
    /// Returns a [`ReadStorage`] implementation backed by Postgres
    async fn access_storage_pg(
        rt_handle: Handle,
        pool: &ConnectionPool,
    ) -> anyhow::Result<PgOrRocksdbStorage> {
        let mut connection = pool.access_storage().await?;

        // Check whether we performed snapshot recovery
        let snapshot_recovery = connection
            .snapshot_recovery_dal()
            .get_applied_snapshot_status()
            .await
            .context("failed getting snapshot recovery info")?;
        let (miniblock_number, l1_batch_number) = if let Some(snapshot_recovery) = snapshot_recovery
        {
            (
                snapshot_recovery.miniblock_number,
                snapshot_recovery.l1_batch_number,
            )
        } else {
            let mut dal = connection.blocks_dal();
            let l1_batch_number = dal.get_sealed_l1_batch_number().await?.unwrap_or_default();
            let (_, miniblock_number) = dal
                .get_miniblock_range_of_l1_batch(l1_batch_number)
                .await?
                .unwrap_or_default();
            (miniblock_number, l1_batch_number)
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
        let rocksdb_builder = RocksdbStorage::builder(rocksdb)
            .await
            .context("Failed initializing RocksDB storage")?;
        let rocksdb = rocksdb_builder
            .synchronize(conn, stop_receiver)
            .await
            .context("Failed to catch up state keeper RocksDB storage to Postgres")?;
        if let Some(rocksdb) = &rocksdb {
            let rocksdb_l1_batch_number = rocksdb.l1_batch_number().await.unwrap_or_default();
            tracing::debug!(%rocksdb_l1_batch_number, "Using RocksDB-based storage");
        } else {
            tracing::warn!("Interrupted")
        }
        Ok(rocksdb.map(|rocksdb| rocksdb.into()))
    }

    async fn access_storage_inner<'a>(
        &'a self,
        rt_handle: Handle,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Option<PgOrRocksdbStorage<'a>> {
        match self {
            AsyncRocksdbCache::Postgres(pool) => Some(
                Self::access_storage_pg(rt_handle, pool)
                    .await
                    .expect("Failed accessing Postgres storage"),
            ),
            AsyncRocksdbCache::Rocksdb(rocksdb, pool) => {
                let mut conn = pool
                    .access_storage_tagged("state_keeper")
                    .await
                    .expect("Failed getting a Postgres connection");
                Self::access_storage_rocksdb(&mut conn, rocksdb.clone(), stop_receiver)
                    .await
                    .expect("Failed accessing RocksDB storage")
            }
        }
    }
}

#[async_trait]
impl ReadStorageFactory for AsyncRocksdbCache {
    type ReadStorageImpl<'a> = PgOrRocksdbStorage<'a>;

    async fn access_storage<'a>(
        &'a self,
        rt_handle: Handle,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Option<Self::ReadStorageImpl<'a>> {
        self.access_storage_inner(rt_handle, stop_receiver).await
    }
}

impl<T: ReadStorageFactory> StateKeeperStorage<T> {
    pub fn new(inner: Arc<Mutex<T>>) -> StateKeeperStorage<T> {
        Self { inner }
    }

    pub fn factory(&self) -> T {
        // It's important that we don't hold the lock for long; that's why `T` implements `Clone`
        self.inner.lock().expect("poisoned").clone()
    }
}

impl StateKeeperStorage<AsyncRocksdbCache> {
    pub fn async_rocksdb_cache(
        pool: ConnectionPool,
        state_keeper_db_path: String,
        enum_index_migration_chunk_size: usize,
    ) -> Self {
        let inner = Arc::new(Mutex::new(AsyncRocksdbCache::Postgres(pool.clone())));
        let factory = inner.clone();
        tokio::task::spawn(async move {
            tracing::debug!("Catching up RocksDB asynchronously");
            let mut rocksdb_builder = RocksdbStorage::open_builder(state_keeper_db_path.as_ref())
                .await
                .expect("Failed initializing RocksDB storage");
            rocksdb_builder.enable_enum_index_migration(enum_index_migration_chunk_size);
            let mut storage = pool
                .access_storage()
                .await
                .expect("Failed accessing Postgres storage");
            let (_, stop_receiver) = watch::channel(false);
            let rocksdb = rocksdb_builder
                .synchronize(&mut storage, &stop_receiver)
                .await
                .expect("Failed to catch up RocksDB to Postgres");
            drop(storage);
            if let Some(rocksdb) = rocksdb {
                let mut factory_guard = factory.lock().expect("poisoned");
                *factory_guard = AsyncRocksdbCache::Rocksdb(rocksdb.db, pool)
            } else {
                tracing::warn!("Interrupted");
            }
        });
        Self::new(inner)
    }
}
