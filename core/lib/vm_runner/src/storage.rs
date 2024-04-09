use std::marker::PhantomData;
use std::{fmt::Debug, sync::Arc};

use anyhow::Context as _;
use async_trait::async_trait;
use once_cell::sync::OnceCell;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core};
use zksync_state::{
    AsyncCatchupTask, PgOrRocksdbStorage, ReadStorageFactory, StateKeeperColumnFamily,
};
use zksync_storage::RocksDB;
use zksync_types::{block::MiniblockExecutionData, L1BatchNumber};

/// Data needed to re-execute an L1 batch.
#[derive(Debug)]
pub struct BatchData {
    /// L1 batch number this data belongs to.
    pub l1_batch_number: L1BatchNumber,
    /// List of miniblocks and corresponding transactions that were executed within batch.
    pub miniblocks: Vec<MiniblockExecutionData>,
}

/// Functionality to fetch data about processed/unprocessed batches for a particular VM runner
/// instance.
#[async_trait]
pub trait VmRunnerStorageLoader: Debug + Send + Sync + 'static {
    /// Unique name of the VM runner instance.
    fn name() -> &'static str;

    /// Loads next unprocessed L1 batch along with all transactions that VM runner needs to
    /// re-execute. These are the transactions that are included in a sealed miniblock belonging
    /// to a sealed L1 batch (with state keeper being the source of truth). The order of the
    /// transactions is the same as it was when state keeper executed them.
    ///
    /// Can return `None` if there are no batches to be processed.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    async fn load_next_batch(conn: Connection<'_, Core>) -> anyhow::Result<Option<BatchData>>;

    /// Returns the last L1 batch number that has been processed by this VM runner instance.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    async fn latest_processed_batch(conn: Connection<'_, Core>) -> anyhow::Result<L1BatchNumber>;
}

/// Abstraction for VM runner's storage layer that provides two main features:
///
/// 1. A [`ReadStorageFactory`] implementation backed by either Postgres or RocksDB (if it's
/// caught up). Always initialized as a `Postgres` variant and is then mutated into `Rocksdb`
/// once RocksDB cache is caught up.
/// 2. Loads data needed to re-execute the next unprocessed L1 batch.
#[derive(Debug)]
pub struct VmRunnerStorage<L: VmRunnerStorageLoader> {
    pool: ConnectionPool<Core>,
    rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
    _marker: PhantomData<L>,
}

impl<L: VmRunnerStorageLoader> VmRunnerStorage<L> {
    /// Creates a new VM runner storage using provided Postgres pool and RocksDB path.
    pub async fn new(
        pool: ConnectionPool<Core>,
        rocksdb_path: String,
        enum_index_migration_chunk_size: usize, // TODO: Remove
    ) -> anyhow::Result<(Self, AsyncCatchupTask)> {
        let rocksdb_cell = Arc::new(OnceCell::new());
        let task = AsyncCatchupTask::new(
            pool.clone(),
            rocksdb_path,
            enum_index_migration_chunk_size,
            rocksdb_cell.clone(),
            Some(L::latest_processed_batch(pool.connection().await?).await?),
        );
        Ok((
            Self {
                pool,
                rocksdb_cell,
                _marker: PhantomData,
            },
            task,
        ))
    }

    async fn access_storage_inner(
        &self,
        stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        if let Some(rocksdb) = self.rocksdb_cell.get() {
            let mut connection = self
                .pool
                .connection_tagged(L::name())
                .await
                .context("Failed getting a Postgres connection")?;
            PgOrRocksdbStorage::access_storage_rocksdb(
                &mut connection,
                rocksdb.clone(),
                stop_receiver,
                l1_batch_number,
            )
            .await
            .context("Failed accessing RocksDB storage")
        } else {
            Ok(Some(
                PgOrRocksdbStorage::access_storage_pg(&self.pool, l1_batch_number)
                    .await
                    .context("Failed accessing Postgres storage")?,
            ))
        }
    }

    /// Loads next unprocessed L1 batch along with all transactions that VM runner needs to
    /// re-execute. These are the transactions that are included in a sealed miniblock belonging
    /// to a sealed L1 batch (with state keeper being the source of truth). The order of the
    /// transactions is the same as it was when state keeper executed them.
    ///
    /// Can return `None` if there are no batches to be processed.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    pub async fn load_next_batch(&self) -> anyhow::Result<Option<BatchData>> {
        let conn = self.pool.connection().await?;
        L::load_next_batch(conn).await
    }
}

#[async_trait]
impl<L: VmRunnerStorageLoader> ReadStorageFactory for VmRunnerStorage<L> {
    async fn access_storage(
        &self,
        stop_receiver: &watch::Receiver<bool>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<PgOrRocksdbStorage<'_>>> {
        self.access_storage_inner(stop_receiver, l1_batch_number)
            .await
    }
}
