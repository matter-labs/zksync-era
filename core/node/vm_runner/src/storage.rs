use std::{
    collections::{BTreeMap, HashMap},
    fmt,
    sync::Arc,
    time::Duration,
};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::sync::{watch, RwLock};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_state::{
    AsyncCatchupTask, BatchDiff, OwnedStorage, RocksdbCell, RocksdbStorage, RocksdbWithMemory,
};
use zksync_types::{
    block::L2BlockExecutionData, commitment::PubdataParams, L1BatchNumber, L2ChainId,
};
use zksync_vm_executor::storage::L1BatchParamsProvider;
use zksync_vm_interface::{L1BatchEnv, SystemEnv};

use crate::{metrics::METRICS, VmRunnerIo};

#[async_trait]
pub trait StorageLoader: 'static + Send + Sync + fmt::Debug {
    /// Loads next unprocessed L1 batch along with all transactions that VM runner needs to
    /// re-execute. These are the transactions that are included in a sealed L2 block belonging
    /// to a sealed L1 batch (with state keeper being the source of truth). The order of the
    /// transactions is the same as it was when state keeper executed them.
    ///
    /// Can return `None` if the requested batch is not available yet.
    ///
    /// # Errors
    ///
    /// Propagates DB errors.
    async fn load_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<(BatchExecuteData, OwnedStorage)>>;
}

/// Simplified storage loader that always gets data from Postgres (i.e., doesn't do RocksDB caching).
#[derive(Debug)]
pub(crate) struct PostgresLoader {
    pool: ConnectionPool<Core>,
    l1_batch_params_provider: L1BatchParamsProvider,
    chain_id: L2ChainId,
    shadow_snapshots: bool,
}

impl PostgresLoader {
    pub async fn new(pool: ConnectionPool<Core>, chain_id: L2ChainId) -> anyhow::Result<Self> {
        let mut conn = pool.connection_tagged("vm_runner").await?;
        let l1_batch_params_provider = L1BatchParamsProvider::new(&mut conn).await?;
        Ok(Self {
            pool,
            l1_batch_params_provider,
            chain_id,
            shadow_snapshots: true,
        })
    }

    /// Enables or disables snapshot storage shadowing.
    pub fn shadow_snapshots(&mut self, shadow_snapshots: bool) {
        self.shadow_snapshots = shadow_snapshots;
    }
}

#[async_trait]
impl StorageLoader for PostgresLoader {
    #[tracing::instrument(skip_all, l1_batch_number = l1_batch_number.0)]
    async fn load_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<(BatchExecuteData, OwnedStorage)>> {
        let mut conn = self.pool.connection_tagged("vm_runner").await?;
        let Some(data) = load_batch_execute_data(
            &mut conn,
            l1_batch_number,
            &self.l1_batch_params_provider,
            self.chain_id,
        )
        .await?
        else {
            return Ok(None);
        };

        if let Some(snapshot) = OwnedStorage::snapshot(&mut conn, l1_batch_number).await? {
            let postgres = OwnedStorage::postgres(conn, l1_batch_number - 1).await?;
            let storage = snapshot.with_fallback(postgres.into(), self.shadow_snapshots);
            let storage = OwnedStorage::from(storage);
            return Ok(Some((data, storage)));
        }

        tracing::info!(
            "Incomplete data to create storage snapshot for batch; will use sequential storage"
        );
        let conn = self.pool.connection_tagged("vm_runner").await?;
        let storage = OwnedStorage::postgres(conn, l1_batch_number - 1).await?;
        Ok(Some((data, storage.into())))
    }
}

/// Data needed to execute an L1 batch.
#[derive(Debug, Clone)]
pub struct BatchExecuteData {
    /// Parameters for L1 batch this data belongs to.
    pub l1_batch_env: L1BatchEnv,
    /// Execution process parameters.
    pub system_env: SystemEnv,
    /// Pubdata building parameters.
    pub pubdata_params: PubdataParams,
    /// List of L2 blocks and corresponding transactions that were executed within batch.
    pub l2_blocks: Vec<L2BlockExecutionData>,
}

#[derive(Debug, Clone)]
struct BatchData {
    execute_data: BatchExecuteData,
    diff: BatchDiff,
}

/// Abstraction for VM runner's storage layer that provides two main features:
///
/// 1. A [`ReadStorageFactory`] implementation backed by either Postgres or RocksDB (if it's
///    caught up). Always initialized as a `Postgres` variant and is then mutated into `Rocksdb`
///    once RocksDB cache is caught up.
/// 2. Loads data needed to re-execute the next unprocessed L1 batch.
///
/// Users of `VmRunnerStorage` are not supposed to retain storage access to batches that are less
/// than `L::latest_processed_batch`. Holding one is considered to be an undefined behavior.
#[derive(Debug)]
pub struct VmRunnerStorage<Io: VmRunnerIo> {
    pool: ConnectionPool<Core>,
    l1_batch_params_provider: L1BatchParamsProvider,
    chain_id: L2ChainId,
    state: Arc<RwLock<State>>,
    io: Io,
}

#[derive(Debug)]
struct State {
    rocksdb: Option<RocksdbStorage>,
    l1_batch_number: L1BatchNumber,
    storage: BTreeMap<L1BatchNumber, BatchData>,
}

impl<Io: VmRunnerIo + Clone> VmRunnerStorage<Io> {
    /// Creates a new VM runner storage using provided Postgres pool and RocksDB path.
    pub async fn new(
        pool: ConnectionPool<Core>,
        rocksdb_path: String,
        io: Io,
        chain_id: L2ChainId,
    ) -> anyhow::Result<(Self, StorageSyncTask<Io>)> {
        let mut conn = pool.connection_tagged(io.name()).await?;
        let l1_batch_params_provider = L1BatchParamsProvider::new(&mut conn)
            .await
            .context("Failed initializing L1 batch params provider")?;
        drop(conn);

        let state = Arc::new(RwLock::new(State {
            rocksdb: None,
            l1_batch_number: L1BatchNumber(0),
            storage: BTreeMap::new(),
        }));
        let task = StorageSyncTask::new(
            pool.clone(),
            chain_id,
            rocksdb_path,
            io.clone(),
            state.clone(),
        )
        .await?;
        Ok((
            Self {
                pool,
                l1_batch_params_provider,
                chain_id,
                state,
                io,
            },
            task,
        ))
    }
}

#[async_trait]
impl<Io: VmRunnerIo> StorageLoader for VmRunnerStorage<Io> {
    async fn load_batch(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Option<(BatchExecuteData, OwnedStorage)>> {
        let state = self.state.read().await;
        let rocksdb = if let Some(rocksdb) = &state.rocksdb {
            rocksdb
        } else {
            drop(state);
            let mut conn = self.pool.connection_tagged(self.io.name()).await?;
            let batch_data = load_batch_execute_data(
                &mut conn,
                l1_batch_number,
                &self.l1_batch_params_provider,
                self.chain_id,
            )
            .await?;

            return Ok(if let Some(data) = batch_data {
                let storage = OwnedStorage::postgres(conn, l1_batch_number - 1).await?;
                Some((data, storage.into()))
            } else {
                None
            });
        };

        match state.storage.get(&l1_batch_number) {
            None => {
                tracing::debug!(
                    %l1_batch_number,
                    min_l1_batch = %(state.l1_batch_number + 1),
                    max_l1_batch = %state.storage.last_key_value().map(|(k, _)| *k).unwrap_or(state.l1_batch_number),
                    "Trying to load an L1 batch that is not available"
                );
                Ok(None)
            }
            Some(batch_data) => {
                let data = batch_data.execute_data.clone();
                let batch_diffs = state
                    .storage
                    .iter()
                    .filter(|(&num, _)| num < l1_batch_number)
                    .map(|(_, data)| data.diff.clone())
                    .collect::<Vec<_>>();
                let storage = OwnedStorage::RocksdbWithMemory(RocksdbWithMemory {
                    rocksdb: rocksdb.clone(),
                    batch_diffs,
                });
                Ok(Some((data, storage)))
            }
        }
    }
}

/// A runnable task that catches up the provided RocksDB cache instance to the latest processed batch.
///
/// Then continuously makes sure that this invariant is held for the foreseeable future.
/// In the meanwhile, `StorageSyncTask` also loads the next `max_batches_to_load` batches in memory
/// so that they are immediately accessible by [`VmRunnerStorage`].
#[derive(Debug)]
pub struct StorageSyncTask<Io: VmRunnerIo> {
    pool: ConnectionPool<Core>,
    l1_batch_params_provider: L1BatchParamsProvider,
    chain_id: L2ChainId,
    rocksdb_cell: RocksdbCell,
    io: Io,
    state: Arc<RwLock<State>>,
    catchup_task: AsyncCatchupTask,
}

impl<Io: VmRunnerIo> StorageSyncTask<Io> {
    async fn new(
        pool: ConnectionPool<Core>,
        chain_id: L2ChainId,
        rocksdb_path: String,
        io: Io,
        state: Arc<RwLock<State>>,
    ) -> anyhow::Result<Self> {
        let mut conn = pool.connection_tagged(io.name()).await?;
        let l1_batch_params_provider = L1BatchParamsProvider::new(&mut conn)
            .await
            .context("Failed initializing L1 batch params provider")?;
        let target_l1_batch_number = io.latest_processed_batch(&mut conn).await?;
        drop(conn);

        let (catchup_task, rocksdb_cell) = AsyncCatchupTask::new(pool.clone(), rocksdb_path);
        Ok(Self {
            pool,
            l1_batch_params_provider,
            chain_id,
            rocksdb_cell,
            io,
            state,
            catchup_task: catchup_task.with_target_l1_batch_number(target_l1_batch_number),
        })
    }

    /// Access the underlying [`VmRunnerIo`].
    pub fn io(&self) -> &Io {
        &self.io
    }

    /// Block until RocksDB cache instance is caught up with Postgres and then continuously makes
    /// sure that the new ready batches are loaded into the cache.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        const SLEEP_INTERVAL: Duration = Duration::from_millis(50);

        self.catchup_task.run(stop_receiver.clone()).await?;
        let mut rocksdb = self.rocksdb_cell.wait().await?;
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("`StorageSyncTask` was interrupted");
                return Ok(());
            }
            let mut conn = self.pool.connection_tagged(self.io.name()).await?;
            let latest_processed_batch = self.io.latest_processed_batch(&mut conn).await?;
            if rocksdb.next_l1_batch_number().await == latest_processed_batch + 1 {
                // RocksDB is already caught up, we might not need to do anything.
                // Just need to check that the memory diff is up-to-date in case this is a fresh start.
                let last_ready_batch = self.io.last_ready_to_be_loaded_batch(&mut conn).await?;
                let state = self.state.read().await;
                if last_ready_batch == latest_processed_batch
                    || state.storage.contains_key(&last_ready_batch)
                {
                    // No need to do anything, killing time until last processed batch is updated.
                    drop(conn);
                    drop(state);
                    tokio::time::sleep(SLEEP_INTERVAL).await;
                    continue;
                }
            }
            // We rely on the assumption that no one is holding storage access to a batch with
            // number less than `latest_processed_batch`. If they do, RocksDB synchronization below
            // will cause them to have an inconsistent view on DB which we consider to be an
            // undefined behavior.
            let Some(updated_rocksdb) = rocksdb
                .synchronize(&mut conn, &stop_receiver, Some(latest_processed_batch))
                .await
                .context("Failed to catch up state keeper RocksDB storage to Postgres")?
            else {
                tracing::info!("`StorageSyncTask` was interrupted during RocksDB synchronization");
                return Ok(());
            };
            rocksdb = updated_rocksdb;

            let mut state = self.state.write().await;
            state.rocksdb = Some(rocksdb.clone());
            state.l1_batch_number = latest_processed_batch;
            state
                .storage
                .retain(|l1_batch_number, _| l1_batch_number > &latest_processed_batch);
            let max_present = state
                .storage
                .last_entry()
                .map(|e| *e.key())
                .unwrap_or(latest_processed_batch);
            drop(state);
            let max_desired = self.io.last_ready_to_be_loaded_batch(&mut conn).await?;
            for l1_batch_number in max_present.0 + 1..=max_desired.0 {
                let latency = METRICS.storage_load_time.start();
                let l1_batch_number = L1BatchNumber(l1_batch_number);
                let Some(execute_data) = load_batch_execute_data(
                    &mut conn,
                    l1_batch_number,
                    &self.l1_batch_params_provider,
                    self.chain_id,
                )
                .await?
                else {
                    break;
                };
                let state_diff = conn
                    .storage_logs_dal()
                    .get_touched_slots_for_l1_batch(l1_batch_number)
                    .await?;
                let enum_index_diff = conn
                    .storage_logs_dedup_dal()
                    .initial_writes_for_batch(l1_batch_number)
                    .await?
                    .into_iter()
                    .collect::<HashMap<_, _>>();
                let factory_dep_diff = conn
                    .blocks_dal()
                    .get_l1_batch_factory_deps(l1_batch_number)
                    .await?;
                let diff = BatchDiff {
                    state_diff,
                    enum_index_diff,
                    factory_dep_diff,
                };

                let mut state = self.state.write().await;
                state
                    .storage
                    .insert(l1_batch_number, BatchData { execute_data, diff });
                drop(state);
                latency.observe();
            }
            drop(conn);
        }
    }
}

pub(crate) async fn load_batch_execute_data(
    conn: &mut Connection<'_, Core>,
    l1_batch_number: L1BatchNumber,
    l1_batch_params_provider: &L1BatchParamsProvider,
    chain_id: L2ChainId,
) -> anyhow::Result<Option<BatchExecuteData>> {
    let Some((system_env, l1_batch_env, pubdata_params)) = l1_batch_params_provider
        .load_l1_batch_env(
            conn,
            l1_batch_number,
            // `validation_computational_gas_limit` is only relevant when rejecting txs, but we
            // are re-executing so none of them should be rejected
            u32::MAX,
            chain_id,
        )
        .await?
    else {
        return Ok(None);
    };

    let l2_blocks = conn
        .transactions_dal()
        .get_l2_blocks_to_execute_for_l1_batch(l1_batch_number)
        .await?;
    Ok(Some(BatchExecuteData {
        l1_batch_env,
        system_env,
        pubdata_params,
        l2_blocks,
    }))
}
