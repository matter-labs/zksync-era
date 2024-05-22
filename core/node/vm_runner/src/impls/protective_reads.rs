use crate::storage::StorageSyncTask;
use crate::{
    ConcurrentOutputHandlerFactory, ConcurrentOutputHandlerFactoryTask, OutputHandlerFactory,
    VmRunner, VmRunnerIo, VmRunnerStorage,
};
use anyhow::Context;
use async_trait::async_trait;
use std::sync::Arc;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_state_keeper::{MainBatchExecutor, StateKeeperOutputHandler, UpdatesManager};
use zksync_types::{L1BatchNumber, L2ChainId};

pub struct ProtectiveReadsWriter {
    vm_runner: VmRunner,
}

impl ProtectiveReadsWriter {
    pub async fn new(
        pool: ConnectionPool<Core>,
        rocksdb_path: String,
        chain_id: L2ChainId,
        window_size: u32,
    ) -> anyhow::Result<(Self, ProtectiveReadsWriterTasks)> {
        let io = ProtectiveReadsIo { window_size };
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
}

pub struct ProtectiveReadsWriterTasks {
    pub loader_task: StorageSyncTask<ProtectiveReadsIo>,
    pub output_handler_factory_task: ConcurrentOutputHandlerFactoryTask<ProtectiveReadsIo>,
}

#[derive(Debug, Clone)]
pub struct ProtectiveReadsIo {
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
            .get_protective_reads_latest_processed_batch()
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
        let (_, protective_reads): (Vec<_>, Vec<_>) = finished_batch
            .final_execution_state
            .deduplicated_storage_log_queries
            .iter()
            .partition(|log_query| log_query.rw_flag);

        let mut connection = self
            .pool
            .connection_tagged("protective_reads_writer")
            .await?;
        connection
            .storage_logs_dedup_dal()
            .insert_protective_reads(updates_manager.l1_batch.number, &protective_reads)
            .await?;
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
