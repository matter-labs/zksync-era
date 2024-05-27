use zksync_config::configs::{chain::NetworkConfig, vm_runner::VmRunnerConfig};
use zksync_vm_runner::ProtectiveReadsWriter;

use crate::{
    implementations::resources::pools::{MasterPool, PoolResource},
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct ProtectiveReadsWriterLayer {
    vm_runner_config: VmRunnerConfig,
    network_config: NetworkConfig,
}

impl ProtectiveReadsWriterLayer {
    pub fn new(vm_runner_config: VmRunnerConfig, network_config: NetworkConfig) -> Self {
        Self {
            vm_runner_config,
            network_config,
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
            master_pool.get_singleton().await?, // TODO: check pool size
            self.vm_runner_config.protective_reads_db_path,
            self.network_config.zksync_network_id,
            self.vm_runner_config.protective_reads_window_size,
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
        TaskId("vm_runner/protective_reads_writer".to_owned())
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.protective_reads_writer.run(&stop_receiver.0).await
    }
}
