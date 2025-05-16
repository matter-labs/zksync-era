use zksync_config::configs::vm_runner::ProtectiveReadsWriterConfig;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_types::L2ChainId;

use crate::{
    impls::{ProtectiveReadsIo, ProtectiveReadsWriter},
    ConcurrentOutputHandlerFactoryTask, StorageSyncTask,
};

/// Wiring layer for protective reads writer.
#[derive(Debug)]
pub struct ProtectiveReadsWriterLayer {
    protective_reads_writer_config: ProtectiveReadsWriterConfig,
    zksync_network_id: L2ChainId,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub protective_reads_writer: ProtectiveReadsWriter,
    #[context(task)]
    pub loader_task: StorageSyncTask<ProtectiveReadsIo>,
    #[context(task)]
    pub output_handler_factory_task: ConcurrentOutputHandlerFactoryTask<ProtectiveReadsIo>,
}

impl ProtectiveReadsWriterLayer {
    /// Creates a layer with the provided config.
    pub fn new(
        protective_reads_writer_config: ProtectiveReadsWriterConfig,
        zksync_network_id: L2ChainId,
    ) -> Self {
        Self {
            protective_reads_writer_config,
            zksync_network_id,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ProtectiveReadsWriterLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "vm_runner_protective_reads"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let master_pool = input.master_pool;

        let (protective_reads_writer, tasks) = ProtectiveReadsWriter::new(
            // One for `StorageSyncTask` which can hold a long-term connection in case it needs to
            // catch up cache.
            //
            // One for `ConcurrentOutputHandlerFactoryTask`/`VmRunner` as they need occasional access
            // to DB for querying last processed batch and last ready to be loaded batch.
            //
            // `window_size` connections for `ProtectiveReadsOutputHandlerFactory`
            // as there can be multiple output handlers holding multi-second connections to write
            // large amount of protective reads.
            master_pool
                .get_custom(self.protective_reads_writer_config.window_size + 2)
                .await?,
            self.protective_reads_writer_config.db_path,
            self.zksync_network_id,
            self.protective_reads_writer_config.first_processed_batch,
            self.protective_reads_writer_config.window_size,
        )
        .await?;

        Ok(Output {
            protective_reads_writer,
            loader_task: tasks.loader_task,
            output_handler_factory_task: tasks.output_handler_factory_task,
        })
    }
}

#[async_trait::async_trait]
impl Task for ProtectiveReadsWriter {
    fn id(&self) -> TaskId {
        "vm_runner/protective_reads_writer".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(&stop_receiver.0).await
    }
}
