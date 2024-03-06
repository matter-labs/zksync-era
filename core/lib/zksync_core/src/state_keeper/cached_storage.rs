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

type BoxReadStorage<'a> = Box<dyn ReadStorage + Send + 'a>;

/// Encapsulates a storage that can produce a short-lived [`ReadStorage`] implementation backed by
/// either Postgres or RocksDB (if it's caught up). Maintains internal state of how behind RocksDB
/// is compared to Postgres and actively tries to catch it up in the background.
///
/// This struct's main design purpose is to be able to produce [`ReadStorage`] implementation with
/// as little blocking operations as possible to ensure liveliness.
#[derive(Debug, Clone)]
pub struct CachedStorage<T: ReadStorageFactory> {
    inner: Arc<Mutex<T>>,
}

#[async_trait]
pub trait ReadStorageFactory {
    type ReadStorageImpl<'a>: ReadStorage
    where
        Self: 'a;

    async fn access_storage<'a>(
        &'a self,
        rt_handle: Handle,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Option<Self::ReadStorageImpl<'a>>;
}

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
    ) -> anyhow::Result<BoxReadStorage> {
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
        Ok(Box::new(
            PostgresStorage::new_async(rt_handle, connection, miniblock_number, true).await?,
        ) as BoxReadStorage)
    }

    /// Catches up RocksDB synchronously (i.e. assumes the gap is small) and
    /// returns a [`ReadStorage`] implementation backed by caught-up RocksDB.
    async fn access_storage_rocksdb<'a>(
        conn: &mut StorageProcessor<'_>,
        rocksdb: RocksDB<StateKeeperColumnFamily>,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<BoxReadStorage<'a>>> {
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
        Ok(rocksdb.map(|rocksdb| Box::new(rocksdb) as BoxReadStorage<'a>))
    }

    pub async fn access_storage_inner<'a>(
        &'a self,
        rt_handle: Handle,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Option<BoxReadStorage<'a>> {
        match self {
            AsyncRocksdbCache::Postgres(pool) => {
                Some(Self::access_storage_pg(rt_handle, pool).await.expect(""))
            }
            AsyncRocksdbCache::Rocksdb(rocksdb, pool) => {
                let mut conn = pool
                    .access_storage_tagged("state_keeper")
                    .await
                    .expect("Failed getting a Postgres connection");
                Self::access_storage_rocksdb(&mut conn, rocksdb.clone(), stop_receiver)
                    .await
                    .expect("")
            }
        }
    }
}

#[async_trait]
impl ReadStorageFactory for AsyncRocksdbCache {
    type ReadStorageImpl<'a> = BoxReadStorage<'a>;

    async fn access_storage<'a>(
        &'a self,
        rt_handle: Handle,
        stop_receiver: &watch::Receiver<bool>,
    ) -> Option<Self::ReadStorageImpl<'a>> {
        self.access_storage_inner(rt_handle, stop_receiver).await
    }
}

impl<T: ReadStorageFactory + Clone> CachedStorage<T> {
    pub fn new(inner: Arc<Mutex<T>>) -> CachedStorage<T> {
        Self { inner }
    }

    pub fn factory(&self) -> T {
        // it's important that we don't hold the lock for long;
        // that's why `CachedStorageFactory` implements `Clone`
        self.inner.lock().expect("poisoned").clone()
    }
}

impl CachedStorage<AsyncRocksdbCache> {
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
            let mut storage = pool.access_storage().await.expect("");
            let (_, stop_receiver) = watch::channel(false);
            let rocksdb = rocksdb_builder
                .synchronize(&mut storage, &stop_receiver)
                .await
                .expect("");
            drop(storage);
            if let Some(rocksdb) = rocksdb {
                let mut factory_guard = factory.lock().expect("");
                *factory_guard = AsyncRocksdbCache::Rocksdb(rocksdb.db, pool)
            } else {
                tracing::warn!("Interrupted");
            }
        });
        Self::new(inner)
    }
}
