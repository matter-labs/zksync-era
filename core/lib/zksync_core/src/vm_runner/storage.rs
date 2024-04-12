use std::{fmt::Debug, sync::Arc};

use anyhow::Context as _;
use async_trait::async_trait;
use multivm::{interface::L1BatchEnv, vm_1_4_2::SystemEnv};
use once_cell::sync::OnceCell;
use tokio::sync::watch;
use vm_utils::storage::L1BatchParamsProvider;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_state::{
    AsyncCatchupTask, PgOrRocksdbStorage, ReadStorageFactory, StateKeeperColumnFamily,
};
use zksync_storage::RocksDB;
use zksync_types::{block::MiniblockExecutionData, L1BatchNumber, L2ChainId};

/// Data needed to re-execute an L1 batch.
#[derive(Debug)]
pub struct BatchData {
    /// Parameters for L1 batch this data belongs to.
    pub l1_batch_env: L1BatchEnv,
    /// Execution process parameters.
    pub system_env: SystemEnv,
    /// List of miniblocks and corresponding transactions that were executed within batch.
    pub miniblocks: Vec<MiniblockExecutionData>,
}

/// Functionality to fetch data about processed/unprocessed batches for a particular VM runner
/// instance.
#[async_trait]
pub trait VmRunnerStorageLoader: Debug + Send + Sync + 'static {
    /// Unique name of the VM runner instance.
    fn name() -> &'static str;

    /// Returns number of the next unprocessed L1 batch. Can return `None` if there are no batches
    /// to be processed.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    async fn first_unprocessed_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<Option<L1BatchNumber>>;

    /// Returns the last L1 batch number that has been processed by this VM runner instance.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    async fn latest_processed_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber>;
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
    l1_batch_params_provider: L1BatchParamsProvider,
    chain_id: L2ChainId,
    rocksdb_cell: Arc<OnceCell<RocksDB<StateKeeperColumnFamily>>>,
    loader: L,
}

impl<L: VmRunnerStorageLoader> VmRunnerStorage<L> {
    /// Creates a new VM runner storage using provided Postgres pool and RocksDB path.
    pub async fn new(
        pool: ConnectionPool<Core>,
        rocksdb_path: String,
        loader: L,
        enum_index_migration_chunk_size: usize, // TODO: Remove
        chain_id: L2ChainId,
    ) -> anyhow::Result<(Self, AsyncCatchupTask)> {
        let mut conn = pool.connection_tagged(L::name()).await?;
        let l1_batch_params_provider = L1BatchParamsProvider::new(&mut conn)
            .await
            .context("Failed initializing L1 batch params provider")?;
        let rocksdb_cell = Arc::new(OnceCell::new());
        let task = AsyncCatchupTask::new(
            pool.clone(),
            rocksdb_path,
            enum_index_migration_chunk_size,
            rocksdb_cell.clone(),
            Some(loader.latest_processed_batch(&mut conn).await?),
        );
        drop(conn);
        Ok((
            Self {
                pool,
                l1_batch_params_provider,
                chain_id,
                rocksdb_cell,
                loader,
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
        let mut conn = self.pool.connection().await?;
        let Some(l1_batch_number) = self.loader.first_unprocessed_batch(&mut conn).await? else {
            return Ok(None);
        };
        let first_miniblock_in_batch = self
            .l1_batch_params_provider
            .load_first_miniblock_in_batch(&mut conn, l1_batch_number)
            .await
            .with_context(|| {
                format!(
                    "Аailed loading first miniblock for L1 batch #{}",
                    l1_batch_number
                )
            })?;
        let Some(first_miniblock_in_batch) = first_miniblock_in_batch else {
            anyhow::bail!("Tried to re-execute L1 batch #{l1_batch_number} which does not contain any miniblocks");
        };
        let (system_env, l1_batch_env) = self
            .l1_batch_params_provider
            .load_l1_batch_params(
                &mut conn,
                &first_miniblock_in_batch,
                // validation_computational_gas_limit is only relevant when rejecting txs, but we
                // are re-executing so none of them should be rejected
                u32::MAX,
                self.chain_id,
            )
            .await
            .with_context(|| format!("Failed loading params for L1 batch #{}", l1_batch_number))?;
        let miniblocks = conn
            .transactions_dal()
            .get_miniblocks_to_execute_for_l1_batch(l1_batch_number)
            .await?;
        Ok(Some(BatchData {
            l1_batch_env,
            system_env,
            miniblocks,
        }))
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
