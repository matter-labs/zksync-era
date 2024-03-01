use std::fmt::Debug;

use anyhow::Context;
use tokio::{runtime::Handle, sync::watch, task::JoinHandle};
use zksync_dal::{ConnectionPool, StorageProcessor};
use zksync_state::{PostgresStorage, ReadStorage, RocksdbStorage};
use zksync_types::L1BatchNumber;

type BoxReadStorage<'a> = Box<dyn ReadStorage + Send + 'a>;

/// Encapsulates a storage that can produce a short-lived [`ReadStorage`] implementation backed by
/// either Postgres or RocksDB (if it's caught up). Maintains internal state of how behind RocksDB
/// is compared to Postgres and actively tries to catch it up in the background.
///
/// This struct's main design purpose is to be able to produce [`ReadStorage`] implementation with
/// as little blocking operations as possible to ensure liveliness.
#[derive(Debug)]
pub struct CachedStorage {
    pool: ConnectionPool,
    state_keeper_db_path: String,
    enum_index_migration_chunk_size: usize,
    state: CachedStorageState,
}

#[derive(Debug)]
enum CachedStorageState {
    /// Cached storage has not been initialized yet, the state of RocksDB to Postgres is undefined
    NotInitialized,
    /// RocksDB is trying to catch up to Postgres asynchronously (the task is represented by the
    /// handle contained inside)
    CatchingUp {
        /// Handle owning permission to join on an asynchronous task to catch up RocksDB to
        /// Postgres.
        ///
        /// # `await` value:
        ///
        /// - Returns `Ok(Some(rocksdb))` if the process succeeded and `rocksdb` is caught up
        /// - Returns `Ok(None)` if the process is interrupted
        /// - Returns `Err(err)` for any propagated Postgres/RocksDB error
        rocksdb_sync_handle: JoinHandle<anyhow::Result<Option<RocksdbStorage>>>,
    },
    /// RocksDB has finished catching up in the near past but is not guaranteed to be in complete
    /// sync with Postgres. That being said, except for extreme circumstances the gap should be
    /// small (see [`CATCH_UP_L1_BATCHES_TOLERANCE`]).
    CaughtUp {
        /// Last L1 batch number that RocksDB was caught up to
        rocksdb_l1_batch_number: L1BatchNumber,
    },
}

impl CachedStorage {
    /// Specifies the number of L1 batches we allow for RocksDB to fall behind
    /// Postgres before we stop trying to catch-up synchronously and instead
    /// use Postgres-backed [`ReadStorage`] connections.
    const CATCH_UP_L1_BATCHES_TOLERANCE: u32 = 100;

    pub fn new(
        pool: ConnectionPool,
        state_keeper_db_path: String,
        enum_index_migration_chunk_size: usize,
    ) -> CachedStorage {
        CachedStorage {
            pool,
            state_keeper_db_path,
            enum_index_migration_chunk_size,
            state: CachedStorageState::NotInitialized,
        }
    }

    /// Spawns a new asynchronous task that tries to make RocksDB caught up with Postgres.
    /// The handle to the task is returned as a part of the resulting state.
    async fn start_catch_up(
        pool: ConnectionPool,
        state_keeper_db_path: &str,
        enum_index_migration_chunk_size: usize,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<CachedStorageState> {
        tracing::debug!("Catching up RocksDB asynchronously");
        let mut rocksdb_builder = RocksdbStorage::builder(state_keeper_db_path.as_ref())
            .await
            .context("Failed initializing RocksDB storage")?;
        rocksdb_builder.enable_enum_index_migration(enum_index_migration_chunk_size);
        let rocksdb_sync_handle = tokio::task::spawn(async move {
            let mut storage = pool.access_storage().await?;
            rocksdb_builder
                .synchronize(&mut storage, &stop_receiver)
                .await
        });
        Ok(CachedStorageState::CatchingUp {
            rocksdb_sync_handle,
        })
    }

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
            (
                connection
                    .blocks_dal()
                    .get_sealed_miniblock_number()
                    .await?
                    .unwrap_or_default(),
                connection
                    .blocks_dal()
                    .get_sealed_l1_batch_number()
                    .await?
                    .unwrap_or_default(),
            )
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
        state_keeper_db_path: &str,
        enum_index_migration_chunk_size: usize,
        stop_receiver: &watch::Receiver<bool>,
    ) -> anyhow::Result<Option<BoxReadStorage<'a>>> {
        tracing::debug!("Catching up RocksDB synchronously");
        let mut rocksdb_builder = RocksdbStorage::builder(state_keeper_db_path.as_ref())
            .await
            .context("Failed initializing RocksDB storage")?;
        rocksdb_builder.enable_enum_index_migration(enum_index_migration_chunk_size);
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

    /// Produces a [`ReadStorage`] implementation backed by either Postgres or
    /// RocksDB (if it's caught up).
    ///
    /// # Return value
    ///
    /// Returns `Ok(None)` if the process is interrupted using `stop_receiver`.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    pub async fn access_storage(
        &mut self,
        rt_handle: Handle,
        stop_receiver: watch::Receiver<bool>,
    ) -> anyhow::Result<Option<BoxReadStorage>> {
        // FIXME: This method can potentially be simplified by using recursion but that requires
        // `BoxFuture` and `Pin` which IMO makes the types much more unreadable than as is
        match self.state {
            CachedStorageState::NotInitialized => {
                // Presuming we are behind, (re-)starting catch up process
                self.state = Self::start_catch_up(
                    self.pool.clone(),
                    &self.state_keeper_db_path,
                    self.enum_index_migration_chunk_size,
                    stop_receiver,
                )
                .await?;
                Ok(Some(Self::access_storage_pg(rt_handle, &self.pool).await?))
            }
            CachedStorageState::CatchingUp {
                ref mut rocksdb_sync_handle,
            } => {
                if !rocksdb_sync_handle.is_finished() {
                    // Has not finished catching up yet
                    return Ok(Some(Self::access_storage_pg(rt_handle, &self.pool).await?));
                }
                // RocksDB has finished catching up. Regardless of the outcome, we won't need the
                // handle anymore so it's safe to replace it
                let rocksdb_sync_handle = std::mem::replace(
                    rocksdb_sync_handle,
                    tokio::task::spawn_blocking(|| Ok(None)),
                );
                match rocksdb_sync_handle.await?? {
                    Some(rocksdb_storage) => {
                        // Caught-up successfully
                        self.state = CachedStorageState::CaughtUp {
                            rocksdb_l1_batch_number: rocksdb_storage
                                .l1_batch_number()
                                .await
                                .unwrap_or_default(),
                        };
                        Ok(Some(Self::access_storage_pg(rt_handle, &self.pool).await?))
                    }
                    None => {
                        // Interrupted
                        self.state = CachedStorageState::NotInitialized;
                        Ok(None)
                    }
                }
            }
            CachedStorageState::CaughtUp {
                rocksdb_l1_batch_number,
            } => {
                let mut conn = self
                    .pool
                    .access_storage_tagged("state_keeper")
                    .await
                    .context("Failed getting a Postgres connection")?;
                let Some(postgres_l1_batch_number) = conn
                    .blocks_dal()
                    .get_sealed_l1_batch_number()
                    .await
                    .context("Failed fetching sealed L1 batch number")?
                else {
                    return Self::access_storage_rocksdb(
                        &mut conn,
                        &self.state_keeper_db_path,
                        self.enum_index_migration_chunk_size,
                        &stop_receiver,
                    )
                    .await;
                };
                if rocksdb_l1_batch_number + Self::CATCH_UP_L1_BATCHES_TOLERANCE
                    < postgres_l1_batch_number
                {
                    tracing::warn!(
                        %rocksdb_l1_batch_number,
                        %postgres_l1_batch_number,
                        "RocksDB fell behind Postgres, trying to catch up asynchronously"
                    );
                    self.state = Self::start_catch_up(
                        self.pool.clone(),
                        &self.state_keeper_db_path,
                        self.enum_index_migration_chunk_size,
                        stop_receiver,
                    )
                    .await?;
                    Ok(Some(Self::access_storage_pg(rt_handle, &self.pool).await?))
                } else {
                    Self::access_storage_rocksdb(
                        &mut conn,
                        &self.state_keeper_db_path,
                        self.enum_index_migration_chunk_size,
                        &stop_receiver,
                    )
                    .await
                }
            }
        }
    }
}
