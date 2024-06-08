use std::sync::Arc;

use anyhow::Context;
use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_state_keeper::{MainBatchExecutor, StateKeeperOutputHandler, UpdatesManager};
use zksync_types::{zk_evm_types::LogQuery, AccountTreeId, L1BatchNumber, L2ChainId, StorageKey};
use zksync_utils::u256_to_h256;

use crate::{
    storage::StorageSyncTask, ConcurrentOutputHandlerFactory, ConcurrentOutputHandlerFactoryTask,
    OutputHandlerFactory, VmRunner, VmRunnerIo, VmRunnerStorage,
};

/// A standalone component that writes protective reads asynchronously to state keeper.
#[derive(Debug)]
pub struct ProtectiveReadsWriter {
    vm_runner: VmRunner,
}

impl ProtectiveReadsWriter {
    /// Create a new protective reads writer from the provided DB parameters and window size which
    /// regulates how many batches this component can handle at the same time.
    pub async fn new(
        pool: ConnectionPool<Core>,
        rocksdb_path: String,
        chain_id: L2ChainId,
        first_processed_batch: L1BatchNumber,
        window_size: u32,
    ) -> anyhow::Result<(Self, ProtectiveReadsWriterTasks)> {
        let io = ProtectiveReadsIo {
            first_processed_batch,
            window_size,
        };
        let (loader, loader_task) =
            VmRunnerStorage::new(pool.clone(), rocksdb_path, io.clone(), chain_id).await?;
        let output_handler_factory = ProtectiveReadsOutputHandlerFactory { pool: pool.clone() };
        let (output_handler_factory, output_handler_factory_task) =
            ConcurrentOutputHandlerFactory::new(pool.clone(), io.clone(), output_handler_factory);
        let batch_processor = MainBatchExecutor::new(false, false);
        let vm_runner = VmRunner::new(
            pool,
            Box::new(io),
            Arc::new(loader),
            Box::new(output_handler_factory),
            Box::new(batch_processor),
        );
        Ok((
            Self { vm_runner },
            ProtectiveReadsWriterTasks {
                loader_task,
                output_handler_factory_task,
            },
        ))
    }

    /// Continuously loads new available batches and writes the corresponding protective reads
    /// produced by that batch.
    ///
    /// # Errors
    ///
    /// Propagates RocksDB and Postgres errors.
    pub async fn run(self, stop_receiver: &watch::Receiver<bool>) -> anyhow::Result<()> {
        self.vm_runner.run(stop_receiver).await
    }
}

/// A collections of tasks that need to be run in order for protective reads writer to work as
/// intended.
#[derive(Debug)]
pub struct ProtectiveReadsWriterTasks {
    /// Task that synchronizes storage with new available batches.
    pub loader_task: StorageSyncTask<ProtectiveReadsIo>,
    /// Task that handles output from processed batches.
    pub output_handler_factory_task: ConcurrentOutputHandlerFactoryTask<ProtectiveReadsIo>,
}

#[derive(Debug, Clone)]
pub struct ProtectiveReadsIo {
    first_processed_batch: L1BatchNumber,
    window_size: u32,
}

#[async_trait]
impl VmRunnerIo for ProtectiveReadsIo {
    fn name(&self) -> &'static str {
        "protective_reads_writer"
    }

    async fn latest_processed_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(conn
            .vm_runner_dal()
            .get_protective_reads_latest_processed_batch(self.first_processed_batch)
            .await?)
    }

    async fn last_ready_to_be_loaded_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(conn
            .vm_runner_dal()
            .get_protective_reads_last_ready_batch(self.window_size)
            .await?)
    }

    async fn mark_l1_batch_as_completed(
        &self,
        conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        Ok(conn
            .vm_runner_dal()
            .mark_protective_reads_batch_as_completed(l1_batch_number)
            .await?)
    }
}

#[derive(Debug)]
struct ProtectiveReadsOutputHandler {
    pool: ConnectionPool<Core>,
}

#[async_trait]
impl StateKeeperOutputHandler for ProtectiveReadsOutputHandler {
    async fn handle_l2_block(&mut self, _updates_manager: &UpdatesManager) -> anyhow::Result<()> {
        Ok(())
    }

    async fn handle_l1_batch(
        &mut self,
        updates_manager: Arc<UpdatesManager>,
    ) -> anyhow::Result<()> {
        let finished_batch = updates_manager
            .l1_batch
            .finished
            .as_ref()
            .context("L1 batch is not actually finished")?;
        let (_, protective_reads): (Vec<LogQuery>, Vec<LogQuery>) = finished_batch
            .final_execution_state
            .deduplicated_storage_log_queries
            .iter()
            .partition(|log_query| log_query.rw_flag);

        let mut connection = self
            .pool
            .connection_tagged("protective_reads_writer")
            .await?;
        let mut expected_protective_reads = connection
            .storage_logs_dedup_dal()
            .get_protective_reads_for_l1_batch(updates_manager.l1_batch.number)
            .await?;

        for protective_read in protective_reads {
            let address = AccountTreeId::new(protective_read.address);
            let key = u256_to_h256(protective_read.key);
            if !expected_protective_reads.remove(&StorageKey::new(address, key)) {
                tracing::error!(
                    l1_batch_number = %updates_manager.l1_batch.number,
                    address = %protective_read.address,
                    key = %key,
                    "VM runner produced a protective read that did not happen in state keeper"
                );
            }
        }
        for remaining_read in expected_protective_reads {
            tracing::error!(
                l1_batch_number = %updates_manager.l1_batch.number,
                address = %remaining_read.address(),
                key = %remaining_read.key(),
                "State keeper produced a protective read that did not happen in VM runner"
            );
        }

        Ok(())
    }
}

#[derive(Debug)]
struct ProtectiveReadsOutputHandlerFactory {
    pool: ConnectionPool<Core>,
}

#[async_trait]
impl OutputHandlerFactory for ProtectiveReadsOutputHandlerFactory {
    async fn create_handler(
        &mut self,
        _l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<Box<dyn StateKeeperOutputHandler>> {
        Ok(Box::new(ProtectiveReadsOutputHandler {
            pool: self.pool.clone(),
        }))
    }
}
