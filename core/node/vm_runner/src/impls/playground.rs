use std::{
    io,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context as _;
use async_trait::async_trait;
use serde::Serialize;
use tokio::{
    fs,
    sync::{oneshot, watch},
};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_health_check::{Health, HealthStatus, HealthUpdater, ReactiveHealthCheck};
use zksync_state::RocksdbStorage;
use zksync_state_keeper::{MainBatchExecutor, StateKeeperOutputHandler, UpdatesManager};
use zksync_types::{vm::FastVmMode, L1BatchNumber, L2ChainId};

use crate::{
    storage::{PostgresLoader, StorageLoader},
    ConcurrentOutputHandlerFactory, ConcurrentOutputHandlerFactoryTask, OutputHandlerFactory,
    StorageSyncTask, VmRunner, VmRunnerIo, VmRunnerStorage,
};

#[derive(Debug, Serialize)]
struct VmPlaygroundHealth {
    vm_mode: FastVmMode,
    last_processed_batch: L1BatchNumber,
}

impl From<VmPlaygroundHealth> for Health {
    fn from(health: VmPlaygroundHealth) -> Self {
        Health::from(HealthStatus::Ready).with_details(health)
    }
}

/// Options related to the VM playground cursor.
#[derive(Debug)]
pub struct VmPlaygroundCursorOptions {
    /// First batch to be processed by the playground. Only used if there are no processed batches, or if [`Self.reset_state`] is set.
    pub first_processed_batch: L1BatchNumber,
    /// Maximum number of L1 batches to process in parallel.
    pub window_size: NonZeroU32,
    /// If set, reset processing to [`Self.first_processed_batch`].
    pub reset_state: bool,
}

#[derive(Debug)]
struct VmPlaygroundRocksdb {
    path: String,
    loader_task_sender: oneshot::Sender<StorageSyncTask<VmPlaygroundIo>>,
}

/// Virtual machine playground. Does not persist anything in Postgres; instead, keeps an L1 batch cursor as a plain text file in the RocksDB directory
/// (so that the playground doesn't repeatedly process same batches after a restart).
///
/// If the RocksDB directory is not specified, the playground works in the ephemeral mode: it takes all inputs from Postgres, doesn't maintain cache
/// and doesn't persist the processed batch cursor. This is mostly useful for debugging purposes.
#[derive(Debug)]
pub struct VmPlayground {
    pool: ConnectionPool<Core>,
    batch_executor: MainBatchExecutor,
    rocksdb: Option<VmPlaygroundRocksdb>,
    chain_id: L2ChainId,
    io: VmPlaygroundIo,
    output_handler_factory:
        ConcurrentOutputHandlerFactory<VmPlaygroundIo, VmPlaygroundOutputHandler>,
    reset_to_batch: Option<L1BatchNumber>,
}

impl VmPlayground {
    /// Creates a new playground.
    pub async fn new(
        pool: ConnectionPool<Core>,
        vm_mode: FastVmMode,
        rocksdb_path: Option<String>,
        chain_id: L2ChainId,
        cursor: VmPlaygroundCursorOptions,
    ) -> anyhow::Result<(Self, VmPlaygroundTasks)> {
        tracing::info!("Starting VM playground with mode {vm_mode:?}, RocksDB cache path: {rocksdb_path:?}, cursor options: {cursor:?}");
        if rocksdb_path.is_none() {
            tracing::warn!(
                "RocksDB cache is disabled; this can lead to significant performance degradation. Additionally, VM playground progress won't be persisted. \
                If this is not intended, set the cache path in app config"
            );
        }

        let cursor_file_path = rocksdb_path
            .as_deref()
            .map(|path| Path::new(path).join("__vm_playground_cursor"));
        let latest_processed_batch = if let Some(path) = &cursor_file_path {
            VmPlaygroundIo::read_cursor(path).await?
        } else {
            None
        };
        tracing::info!("Latest processed batch: {latest_processed_batch:?}");
        let latest_processed_batch = if cursor.reset_state {
            cursor.first_processed_batch
        } else {
            latest_processed_batch.unwrap_or(cursor.first_processed_batch)
        };

        let mut batch_executor = MainBatchExecutor::new(false, false);
        batch_executor.set_fast_vm_mode(vm_mode);

        let io = VmPlaygroundIo {
            cursor_file_path,
            vm_mode,
            window_size: cursor.window_size.get(),
            latest_processed_batch: Arc::new(watch::channel(latest_processed_batch).0),
            health_updater: Arc::new(ReactiveHealthCheck::new("vm_playground").1),
        };
        let (output_handler_factory, output_handler_factory_task) =
            ConcurrentOutputHandlerFactory::new(
                pool.clone(),
                io.clone(),
                VmPlaygroundOutputHandler,
            );

        let (rocksdb, loader_task) = if let Some(path) = rocksdb_path {
            let (loader_task_sender, loader_task_receiver) = oneshot::channel();
            let rocksdb = VmPlaygroundRocksdb {
                path,
                loader_task_sender,
            };
            let loader_task = VmPlaygroundLoaderTask {
                inner: loader_task_receiver,
            };
            (Some(rocksdb), Some(loader_task))
        } else {
            (None, None)
        };
        let this = Self {
            pool,
            batch_executor,
            rocksdb,
            chain_id,
            io,
            output_handler_factory,
            reset_to_batch: cursor.reset_state.then_some(cursor.first_processed_batch),
        };
        Ok((
            this,
            VmPlaygroundTasks {
                loader_task,
                output_handler_factory_task,
            },
        ))
    }

    /// Returns a health check for this component.
    pub fn health_check(&self) -> ReactiveHealthCheck {
        self.io.health_updater.subscribe()
    }

    #[cfg(test)]
    pub(crate) fn io(&self) -> &VmPlaygroundIo {
        &self.io
    }

    #[tracing::instrument(skip(self), err)]
    async fn reset_rocksdb_cache(&self, last_retained_batch: L1BatchNumber) -> anyhow::Result<()> {
        let Some(rocksdb) = &self.rocksdb else {
            tracing::warn!("No RocksDB path specified; skipping resetting cache");
            return Ok(());
        };

        let builder = RocksdbStorage::builder(rocksdb.path.as_ref()).await?;
        let current_l1_batch = builder.l1_batch_number().await;
        if current_l1_batch <= Some(last_retained_batch) {
            tracing::info!("Resetting RocksDB cache is not required: its current batch #{current_l1_batch:?} is lower than the target");
            return Ok(());
        }

        tracing::info!("Resetting RocksDB cache from batch #{current_l1_batch:?}");
        let mut conn = self.pool.connection_tagged("vm_playground").await?;
        builder.roll_back(&mut conn, last_retained_batch).await
    }

    /// Continuously loads new available batches and writes the corresponding data
    /// produced by that batch.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        if let Some(rocksdb) = &self.rocksdb {
            fs::create_dir_all(&rocksdb.path)
                .await
                .with_context(|| format!("cannot create dir `{}`", rocksdb.path))?;
        }

        if let Some(reset_to_batch) = self.reset_to_batch {
            self.io.health_updater.update(HealthStatus::Affected.into());

            self.reset_rocksdb_cache(reset_to_batch).await?;
            self.io
                .write_cursor(reset_to_batch)
                .await
                .context("failed resetting VM playground state")?;
            tracing::info!("Finished resetting playground state");
        }

        self.io.update_health();

        let loader: Arc<dyn StorageLoader> = if let Some(rocksdb) = self.rocksdb {
            let (loader, loader_task) = VmRunnerStorage::new(
                self.pool.clone(),
                rocksdb.path,
                self.io.clone(),
                self.chain_id,
            )
            .await?;
            rocksdb.loader_task_sender.send(loader_task).ok();
            Arc::new(loader)
        } else {
            let loader = PostgresLoader::new(self.pool.clone(), self.chain_id).await?;
            Arc::new(loader)
        };

        let vm_runner = VmRunner::new(
            self.pool,
            Box::new(self.io),
            loader,
            Box::new(self.output_handler_factory),
            Box::new(self.batch_executor),
        );
        vm_runner.run(&stop_receiver).await
    }
}

/// Loader task for the VM playground.
#[derive(Debug)]
pub struct VmPlaygroundLoaderTask {
    inner: oneshot::Receiver<StorageSyncTask<VmPlaygroundIo>>,
}

impl VmPlaygroundLoaderTask {
    /// Runs a task until a stop signal is received.
    pub async fn run(self, mut stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let task = tokio::select! {
            biased;
            _ = stop_receiver.changed() => return Ok(()),
            res = self.inner => match res {
                Ok(task) => task,
                Err(_) => anyhow::bail!("VM playground stopped before spawning loader task"),
            }
        };
        task.run(stop_receiver).await
    }
}

/// Collection of tasks that need to be run in order for the VM playground to work as intended.
#[derive(Debug)]
pub struct VmPlaygroundTasks {
    /// Task that synchronizes storage with new available batches.
    pub loader_task: Option<VmPlaygroundLoaderTask>,
    /// Task that handles output from processed batches.
    pub output_handler_factory_task: ConcurrentOutputHandlerFactoryTask<VmPlaygroundIo>,
}

/// I/O powering [`VmPlayground`].
#[derive(Debug, Clone)]
pub struct VmPlaygroundIo {
    cursor_file_path: Option<PathBuf>,
    vm_mode: FastVmMode,
    window_size: u32,
    // We don't read this value from the cursor file in the `VmRunnerIo` implementation because reads / writes
    // aren't guaranteed to be atomic.
    latest_processed_batch: Arc<watch::Sender<L1BatchNumber>>,
    health_updater: Arc<HealthUpdater>,
}

impl VmPlaygroundIo {
    async fn read_cursor(cursor_file_path: &Path) -> anyhow::Result<Option<L1BatchNumber>> {
        match fs::read_to_string(cursor_file_path).await {
            Ok(buffer) => {
                let cursor = buffer
                    .parse::<u32>()
                    .with_context(|| format!("invalid cursor value: {buffer}"))?;
                Ok(Some(L1BatchNumber(cursor)))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(anyhow::Error::new(err).context(format!(
                "failed reading VM playground cursor from `{}`",
                cursor_file_path.display()
            ))),
        }
    }

    async fn write_cursor(&self, cursor: L1BatchNumber) -> anyhow::Result<()> {
        let Some(cursor_file_path) = &self.cursor_file_path else {
            return Ok(());
        };
        let buffer = cursor.to_string();
        fs::write(cursor_file_path, buffer).await.with_context(|| {
            format!(
                "failed writing VM playground cursor to `{}`",
                cursor_file_path.display()
            )
        })
    }

    fn update_health(&self) {
        let health = VmPlaygroundHealth {
            vm_mode: self.vm_mode,
            last_processed_batch: *self.latest_processed_batch.borrow(),
        };
        self.health_updater.update(health.into());
    }

    #[cfg(test)]
    pub(crate) fn subscribe_to_completed_batches(&self) -> watch::Receiver<L1BatchNumber> {
        self.latest_processed_batch.subscribe()
    }
}

#[async_trait]
impl VmRunnerIo for VmPlaygroundIo {
    fn name(&self) -> &'static str {
        "vm_playground"
    }

    async fn latest_processed_batch(
        &self,
        _conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(*self.latest_processed_batch.borrow())
    }

    async fn last_ready_to_be_loaded_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        let sealed_l1_batch = conn
            .blocks_dal()
            .get_sealed_l1_batch_number()
            .await?
            .context("no L1 batches in Postgres")?;
        let last_processed_l1_batch = self.latest_processed_batch(conn).await?;
        Ok(sealed_l1_batch.min(last_processed_l1_batch + self.window_size))
    }

    async fn mark_l1_batch_as_processing(
        &self,
        _conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        tracing::info!("Started processing L1 batch #{l1_batch_number}");
        Ok(())
    }

    async fn mark_l1_batch_as_completed(
        &self,
        _conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        tracing::info!("Finished processing L1 batch #{l1_batch_number}");
        self.write_cursor(l1_batch_number).await?;
        // We should only update the in-memory value after the write to the cursor file succeeded.
        self.latest_processed_batch.send_replace(l1_batch_number);
        self.update_health();
        Ok(())
    }
}

#[derive(Debug)]
struct VmPlaygroundOutputHandler;

#[async_trait]
impl StateKeeperOutputHandler for VmPlaygroundOutputHandler {
    async fn handle_l2_block(&mut self, updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        tracing::trace!("Processed L2 block #{}", updates_manager.l2_block.number);
        Ok(())
    }
}

#[async_trait]
impl OutputHandlerFactory for VmPlaygroundOutputHandler {
    async fn create_handler(
        &mut self,
        _l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Box<dyn StateKeeperOutputHandler>> {
        Ok(Box::new(Self))
    }
}
