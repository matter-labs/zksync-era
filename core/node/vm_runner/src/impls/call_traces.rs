use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use tokio::sync::watch;
use zksync_dal::{Connection, ConnectionPool, Core, CoreDal};
use zksync_types::{L1BatchNumber, L2ChainId, ProtocolVersionId, StorageLog};
use zksync_vm_executor::batch::MainBatchExecutorFactory;
use zksync_vm_interface::{Call, L1BatchEnv, L2BlockEnv, SystemEnv};

use crate::{
    storage::StorageSyncTask, ConcurrentOutputHandlerFactory, ConcurrentOutputHandlerFactoryTask,
    L1BatchOutput, L2BlockOutput, OutputHandler, OutputHandlerFactory, VmRunner, VmRunnerIo,
    VmRunnerStorage,
};

/// A standalone component that writes protective reads asynchronously to state keeper.
#[derive(Debug)]
pub struct CallTracesWriter {
    vm_runner: VmRunner,
}

impl CallTracesWriter {
    /// Create a new protective reads writer from the provided DB parameters and window size which
    /// regulates how many batches this component can handle at the same time.
    pub async fn new(
        pool: ConnectionPool<Core>,
        rocksdb_path: PathBuf,
        chain_id: L2ChainId,
        first_processed_batch: L1BatchNumber,
        window_size: u32,
    ) -> anyhow::Result<(Self, CallTracesWriterTasks)> {
        let io = CallTracesIo {
            first_processed_batch,
            window_size,
        };
        let (loader, loader_task) =
            VmRunnerStorage::new(pool.clone(), rocksdb_path, io.clone(), chain_id).await?;
        let output_handler_factory = CallTracesOutputHandlerFactory { pool: pool.clone() };
        let (output_handler_factory, output_handler_factory_task) =
            ConcurrentOutputHandlerFactory::new(pool.clone(), io.clone(), output_handler_factory);
        let batch_processor = MainBatchExecutorFactory::<()>::new(true);
        let vm_runner = VmRunner::new(
            pool,
            Arc::new(io),
            Arc::new(loader),
            Arc::new(output_handler_factory),
            Box::new(batch_processor),
        );
        Ok((
            Self { vm_runner },
            CallTracesWriterTasks {
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
pub struct CallTracesWriterTasks {
    /// Task that synchronizes storage with new available batches.
    pub loader_task: StorageSyncTask<CallTracesIo>,
    /// Task that handles output from processed batches.
    pub output_handler_factory_task: ConcurrentOutputHandlerFactoryTask<CallTracesIo>,
}

/// `VmRunnerIo` implementation for protective reads.
#[derive(Debug, Clone)]
pub struct CallTracesIo {
    first_processed_batch: L1BatchNumber,
    window_size: u32,
}

#[async_trait]
impl VmRunnerIo for CallTracesIo {
    fn name(&self) -> &'static str {
        "protective_reads_writer"
    }

    async fn latest_processed_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(conn
            .vm_runner_dal()
            .get_call_traces_latest_processed_batch()
            .await?
            .unwrap_or(self.first_processed_batch))
    }

    async fn last_ready_to_be_loaded_batch(
        &self,
        conn: &mut Connection<'_, Core>,
    ) -> anyhow::Result<L1BatchNumber> {
        Ok(conn
            .vm_runner_dal()
            .get_call_traces_last_ready_batch(self.first_processed_batch, self.window_size)
            .await?)
    }

    async fn mark_l1_batch_as_processing(
        &self,
        conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        Ok(conn
            .vm_runner_dal()
            .mark_call_traces_batch_as_processing(l1_batch_number)
            .await?)
    }

    async fn mark_l1_batch_as_completed(
        &self,
        conn: &mut Connection<'_, Core>,
        l1_batch_number: L1BatchNumber,
    ) -> anyhow::Result<()> {
        conn.vm_runner_dal()
            .mark_call_traces_batch_as_complete(l1_batch_number)
            .await
    }
}

#[derive(Debug)]
struct CallTracesOutputHandler {
    l1_batch_number: L1BatchNumber,
    pool: ConnectionPool<Core>,
}

#[async_trait]
impl OutputHandler for CallTracesOutputHandler {
    async fn handle_l2_block(
        &mut self,
        protocol_version_id: ProtocolVersionId,
        _env: L2BlockEnv,
        output: &L2BlockOutput,
    ) -> anyhow::Result<()> {
        let mut connection = self.pool.connection_tagged("call_traces_writer").await?;

        let traces = output
            .transactions
            .iter()
            .filter(|(_, res)| !res.call_traces.is_empty())
            .map(|(transaction, res)| {
                (
                    transaction.hash(),
                    Call::new_high_level(
                        transaction.gas_limit().as_u64(),
                        transaction.gas_limit().as_u64() - res.tx_result.refunds.gas_refunded,
                        transaction.execute.value,
                        transaction.execute.calldata.clone(),
                        vec![],
                        res.tx_result.result.revert_reason(),
                        res.call_traces.clone(),
                    ),
                )
            })
            .collect::<Vec<_>>();
        connection
            .transactions_dal()
            .insert_call_traces(traces, protocol_version_id)
            .await?;
        Ok(())
    }

    #[tracing::instrument(
        name = "CallTracesOutputHandler::handle_l1_batch",
        skip_all,
        fields(l1_batch = %self.l1_batch_number)
    )]
    async fn handle_l1_batch(self: Box<Self>, output: Arc<L1BatchOutput>) -> anyhow::Result<()> {
        let l1_batch_number = self.l1_batch_number;
        let (_, computed_protective_reads): (Vec<StorageLog>, Vec<StorageLog>) = output
            .batch
            .final_execution_state
            .deduplicated_storage_logs
            .iter()
            .partition(|log_query| log_query.is_write());

        let mut connection = self
            .pool
            .connection_tagged("protective_reads_writer")
            .await?;
        let mut written_protective_reads = connection
            .storage_logs_dedup_dal()
            .get_protective_reads_for_l1_batch(l1_batch_number)
            .await?;

        if !written_protective_reads.is_empty() {
            tracing::debug!(
                l1_batch_number = %l1_batch_number,
                "Protective reads have already been written, validating"
            );
            for protective_read in computed_protective_reads {
                let address = protective_read.key.address();
                let key = protective_read.key.key();
                if !written_protective_reads.remove(&protective_read.key) {
                    tracing::error!(
                        l1_batch_number = %l1_batch_number,
                        address = %address,
                        key = %key,
                        "VM runner produced a protective read that did not happen in state keeper"
                    );
                }
            }
            for remaining_read in written_protective_reads {
                tracing::error!(
                    l1_batch_number = %l1_batch_number,
                    address = %remaining_read.address(),
                    key = %remaining_read.key(),
                    "State keeper produced a protective read that did not happen in VM runner"
                );
            }
        } else {
            tracing::debug!(
                l1_batch_number = %l1_batch_number,
                "Protective reads have not been written, writing"
            );
            connection
                .storage_logs_dedup_dal()
                .insert_protective_reads(l1_batch_number, &computed_protective_reads)
                .await?;
        }

        Ok(())
    }
}

#[derive(Debug)]
struct CallTracesOutputHandlerFactory {
    pool: ConnectionPool<Core>,
}

#[async_trait]
impl OutputHandlerFactory for CallTracesOutputHandlerFactory {
    async fn create_handler(
        &self,
        _system_env: SystemEnv,
        l1_batch_env: L1BatchEnv,
    ) -> anyhow::Result<Box<dyn OutputHandler>> {
        Ok(Box::new(CallTracesOutputHandler {
            pool: self.pool.clone(),
            l1_batch_number: l1_batch_env.number,
        }))
    }
}
