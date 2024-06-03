use zksync_config::configs::vm_runner::ProtectiveReadsWriterConfig;
use zksync_types::L2ChainId;
use zksync_vm_runner::ProtectiveReadsWriter;

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource},
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct ProtectiveReadsWriterLayer {
    protective_reads_writer_config: ProtectiveReadsWriterConfig,
    zksync_network_id: L2ChainId,
}

impl ProtectiveReadsWriterLayer {
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
    fn layer_name(&self) -> &'static str {
        "vm_runner_protective_reads"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool = context.get_resource::<PoolResource<MasterPool>>().await?;

        let (protective_reads_writer, tasks) = ProtectiveReadsWriter::new(
            master_pool.get_custom(3).await?,
            self.protective_reads_writer_config.protective_reads_db_path,
            self.zksync_network_id,
            self.protective_reads_writer_config
                .protective_reads_window_size,
        )
        .await?;

        context.add_task(Box::new(tasks.loader_task));
        context.add_task(Box::new(tasks.output_handler_factory_task));
        context.add_task(Box::new(ProtectiveReadsWriterTask {
            protective_reads_writer,
        }));
        Ok(())
    }
}

#[derive(Debug)]
struct ProtectiveReadsWriterTask {
    protective_reads_writer: ProtectiveReadsWriter,
}

#[async_trait::async_trait]
impl Task for ProtectiveReadsWriterTask {
    fn id(&self) -> TaskId {
        "vm_runner/protective_reads_writer".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.protective_reads_writer.run(&stop_receiver.0).await
    }
}
