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

pub struct BatchData {
    /// List of miniblocks and corresponding transactions that were executed within batch.
    pub(crate) miniblocks: Vec<MiniblockExecutionData>,
}

#[async_trait]
pub trait VmRunnerStorageLoader: Debug + Send + Sync + 'static {
    /// Loads the next L1 batch data from the database.
    ///
    /// # Errors
    ///
    /// Propagates DB errors. Also returns an error if environment doesn't correspond to a pending L1 batch.
    async fn load_next_batch(conn: Connection<'_, Core>) -> anyhow::Result<BatchData>;

    async fn latest_processed_batch(conn: Connection<'_, Core>) -> anyhow::Result<L1BatchNumber>;
}

#[derive(Debug)]
pub struct VmRunnerStorage<L: VmRunnerStorageLoader> {
    pool: ConnectionPool<Core>,
    rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
    _marker: PhantomData<L>,
}

impl<L: VmRunnerStorageLoader> VmRunnerStorage<L> {
    pub async fn new(
        pool: ConnectionPool<Core>,
        state_keeper_db_path: String,
        enum_index_migration_chunk_size: usize,
    ) -> anyhow::Result<(Self, AsyncCatchupTask)> {
        let rocksdb_cell = Arc::new(OnceCell::new());
        let task = AsyncCatchupTask::new(
            pool.clone(),
            state_keeper_db_path,
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
                .connection_tagged("vm_runner")
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

    pub(crate) async fn load_next_batch(&self) -> anyhow::Result<BatchData> {
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
