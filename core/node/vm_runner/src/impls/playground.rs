use std::{
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use anyhow::Context as _;
use async_trait::async_trait;
use tokio::{
    fs,
    sync::{oneshot, watch},
};
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_state::RocksdbStorage;
use zksync_state_keeper::{BatchExecutor, StateKeeperOutputHandler, UpdatesManager};
use zksync_types::{L1BatchNumber, L2ChainId};

use crate::{
    ConcurrentOutputHandlerFactory, ConcurrentOutputHandlerFactoryTask, OutputHandlerFactory,
    StorageSyncTask, VmRunner, VmRunnerIo, VmRunnerStorage,
};

/// Virtual machine playground. Does not persist anything in Postgres; instead, keeps an L1 batch cursor as a plain text file in the RocksDB directory
/// (so that the playground doesn't repeatedly process same batches after a restart).
#[derive(Debug)]
pub struct VmPlayground {
    pool: ConnectionPool<Core>,
    batch_executor: Box<dyn BatchExecutor>,
    rocksdb_path: String,
    chain_id: L2ChainId,
    // Visible for test purposes
    pub(crate) io: VmPlaygroundIo,
    loader_task_sender: oneshot::Sender<StorageSyncTask<VmPlaygroundIo>>,
    output_handler_factory:
        ConcurrentOutputHandlerFactory<VmPlaygroundIo, VmPlaygroundOutputHandler>,
    reset_to_batch: Option<L1BatchNumber>,
}

impl VmPlayground {
    /// Creates a new playground.
    pub async fn new(
        pool: ConnectionPool<Core>,
        batch_executor: Box<dyn BatchExecutor>,
        rocksdb_path: String,
        chain_id: L2ChainId,
        first_processed_batch: L1BatchNumber,
        reset_state: bool,
    ) -> anyhow::Result<(Self, VmPlaygroundTasks)> {
        tracing::info!(
            "Starting VM playground with executor {batch_executor:?}, first processed batch is #{first_processed_batch} \
             (reset processing: {reset_state:?})"
        );

        let cursor_file_path = Path::new(&rocksdb_path).join("__vm_playground_cursor");
        let io = VmPlaygroundIo {
            first_processed_batch,
            cursor_file_path,
            #[cfg(test)]
            completed_batches_sender: Arc::new(watch::channel(first_processed_batch).0),
        };
        let (output_handler_factory, output_handler_factory_task) =
            ConcurrentOutputHandlerFactory::new(
                pool.clone(),
                io.clone(),
                VmPlaygroundOutputHandler,
            );
        let (loader_task_sender, loader_task_receiver) = oneshot::channel();

        let this = Self {
            pool,
            batch_executor,
            rocksdb_path,
            chain_id,
            io,
            loader_task_sender,
            output_handler_factory,
            reset_to_batch: reset_state.then_some(first_processed_batch),
        };
        Ok((
            this,
            VmPlaygroundTasks {
                loader_task: VmPlaygroundLoaderTask {
                    inner: loader_task_receiver,
                },
                output_handler_factory_task,
            },
        ))
    }

    #[tracing::instrument(skip(self), err)]
    async fn reset_rocksdb_cache(&self, last_retained_batch: L1BatchNumber) -> anyhow::Result<()> {
        let builder = RocksdbStorage::builder(self.rocksdb_path.as_ref()).await?;
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
    pub async fn run(self, stop_receiver: &watch::Receiver<bool>) -> anyhow::Result<()> {
        fs::create_dir_all(&self.rocksdb_path)
            .await
            .with_context(|| format!("cannot create dir `{}`", self.rocksdb_path))?;

        if let Some(reset_to_batch) = self.reset_to_batch {
            self.reset_rocksdb_cache(reset_to_batch).await?;
            self.io
                .write_cursor(reset_to_batch)
                .await
                .context("failed resetting VM playground state")?;
            tracing::info!("Finished resetting playground state");
        }

        let (loader, loader_task) = VmRunnerStorage::new(
            self.pool.clone(),
            self.rocksdb_path,
            self.io.clone(),
            self.chain_id,
        )
        .await?;
        self.loader_task_sender.send(loader_task).ok();
        let vm_runner = VmRunner::new(
            self.pool,
            Box::new(self.io),
            Arc::new(loader),
            Box::new(self.output_handler_factory),
            self.batch_executor,
        );
        vm_runner.run(stop_receiver).await
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
    pub loader_task: VmPlaygroundLoaderTask,
    /// Task that handles output from processed batches.
    pub output_handler_factory_task: ConcurrentOutputHandlerFactoryTask<VmPlaygroundIo>,
}

// FIXME: non-atomic file writes.
/// I/O powering [`VmPlayground`].
#[derive(Debug, Clone)]
pub struct VmPlaygroundIo {
    first_processed_batch: L1BatchNumber,
    cursor_file_path: PathBuf,
    #[cfg(test)]
    completed_batches_sender: Arc<watch::Sender<L1BatchNumber>>,
}

impl VmPlaygroundIo {
    async fn read_cursor(&self) -> anyhow::Result<Option<L1BatchNumber>> {
        match fs::read_to_string(&self.cursor_file_path).await {
            Ok(buffer) => {
                let cursor = buffer
                    .parse::<u32>()
                    .with_context(|| format!("invalid cursor value: {buffer}"))?;
                Ok(Some(L1BatchNumber(cursor)))
            }
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(anyhow::Error::new(err).context(format!(
                "failed reading VM playground cursor from `{}`",
                self.cursor_file_path.display()
            ))),
        }
    }

    async fn write_cursor(&self, cursor: L1BatchNumber) -> anyhow::Result<()> {
        let buffer = cursor.to_string();
        fs::write(&self.cursor_file_path, buffer)
            .await
            .with_context(|| {
                format!(
                    "failed writing VM playground cursor to `{}`",
                    self.cursor_file_path.display()
                )
            })
    }

    #[cfg(test)]
    pub(crate) fn subscribe_to_completed_batches(&self) -> watch::Receiver<L1BatchNumber> {
        self.completed_batches_sender.subscribe()
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
        Ok(self
            .read_cursor()
            .await?
            .unwrap_or(self.first_processed_batch))
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
        Ok(sealed_l1_batch.min(last_processed_l1_batch + 1))
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

        #[cfg(test)]
        self.completed_batches_sender.send_replace(l1_batch_number);
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
