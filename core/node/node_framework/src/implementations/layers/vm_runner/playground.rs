use async_trait::async_trait;
use zksync_config::configs::ExperimentalVmPlaygroundConfig;
use zksync_node_framework_derive::{FromContext, IntoContext};
use zksync_types::L2ChainId;
use zksync_vm_runner::{
    impls::{VmPlayground, VmPlaygroundIo, VmPlaygroundLoaderTask},
    ConcurrentOutputHandlerFactoryTask,
};

use crate::{
    implementations::resources::{
        healthcheck::AppHealthCheckResource,
        pools::{MasterPool, PoolResource},
    },
    StopReceiver, Task, TaskId, WiringError, WiringLayer,
};

#[derive(Debug)]
pub struct VmPlaygroundLayer {
    config: ExperimentalVmPlaygroundConfig,
    zksync_network_id: L2ChainId,
}

impl VmPlaygroundLayer {
    pub fn new(config: ExperimentalVmPlaygroundConfig, zksync_network_id: L2ChainId) -> Self {
        Self {
            config,
            zksync_network_id,
        }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    #[context(default)]
    pub app_health: AppHealthCheckResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub output_handler_factory_task: ConcurrentOutputHandlerFactoryTask<VmPlaygroundIo>,
    #[context(task)]
    pub loader_task: VmPlaygroundLoaderTask,
    #[context(task)]
    pub playground: VmPlayground,
}

#[async_trait]
impl WiringLayer for VmPlaygroundLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "vm_runner_playground"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let Input {
            master_pool,
            app_health,
        } = input;

        // - 1 connection for `StorageSyncTask` which can hold a long-term connection in case it needs to
        //   catch up cache.
        // - 1 connection for `ConcurrentOutputHandlerFactoryTask` / `VmRunner` as they need occasional access
        //   to DB for querying last processed batch and last ready to be loaded batch.
        // - 1 connection for the only running VM instance.
        let connection_pool = master_pool.get_custom(3).await?;

        let (playground, tasks) = VmPlayground::new(
            connection_pool,
            self.config.fast_vm_mode,
            self.config.db_path,
            self.zksync_network_id,
            self.config.first_processed_batch,
            self.config.reset,
        )
        .await?;

        app_health
            .0
            .insert_component(playground.health_check())
            .map_err(WiringError::internal)?;

        Ok(Output {
            output_handler_factory_task: tasks.output_handler_factory_task,
            loader_task: tasks.loader_task,
            playground,
        })
    }
}

#[async_trait]
impl Task for VmPlaygroundLoaderTask {
    fn id(&self) -> TaskId {
        "vm_runner/playground/storage_sync".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}

#[async_trait]
impl Task for VmPlayground {
    fn id(&self) -> TaskId {
        "vm_runner/playground".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(&stop_receiver.0).await
    }
}
