use std::sync::Arc;

use async_trait::async_trait;
use zksync_config::configs::ExperimentalVmPlaygroundConfig;
use zksync_dal::node::{PoolResource, ReplicaPool};
use zksync_health_check::AppHealthCheck;
use zksync_node_framework::{
    FromContext, IntoContext, StopReceiver, Task, TaskId, WiringError, WiringLayer,
};
use zksync_object_store::ObjectStore;
use zksync_types::L2ChainId;

use crate::{
    impls::{
        VmPlayground, VmPlaygroundCursorOptions, VmPlaygroundIo, VmPlaygroundLoaderTask,
        VmPlaygroundStorageOptions,
    },
    ConcurrentOutputHandlerFactoryTask,
};

/// Wiring layer for the VM playground.
#[derive(Debug)]
pub struct VmPlaygroundLayer {
    config: ExperimentalVmPlaygroundConfig,
    zksync_network_id: L2ChainId,
}

impl VmPlaygroundLayer {
    /// Creates a layer with the provided config.
    pub fn new(config: ExperimentalVmPlaygroundConfig, zksync_network_id: L2ChainId) -> Self {
        Self {
            config,
            zksync_network_id,
        }
    }
}

#[derive(Debug, FromContext)]
pub struct Input {
    // We use a replica pool because VM playground doesn't write anything to the DB by design.
    replica_pool: PoolResource<ReplicaPool>,
    dumps_object_store: Option<Arc<dyn ObjectStore>>,
    #[context(default)]
    app_health: Arc<AppHealthCheck>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    output_handler_factory_task: ConcurrentOutputHandlerFactoryTask<VmPlaygroundIo>,
    #[context(task)]
    loader_task: Option<VmPlaygroundLoaderTask>,
    #[context(task)]
    playground: VmPlayground,
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
            replica_pool,
            dumps_object_store,
            app_health,
        } = input;

        // - 1 connection for `StorageSyncTask` which can hold a long-term connection in case it needs to
        //   catch up cache.
        // - 1 connection for `ConcurrentOutputHandlerFactoryTask` / `VmRunner` as they need occasional access
        //   to DB for querying last processed batch and last ready to be loaded batch.
        // - `window_size` connections for running VM instances.
        let connection_pool = replica_pool
            .build(|builder| {
                builder
                    .set_max_size(2 + self.config.window_size.get())
                    .set_statement_timeout(None);
                // Unlike virtually all other replica pool uses, VM playground has some long-living operations,
                // so the default statement timeout would only get in the way.
            })
            .await?;

        let cursor = VmPlaygroundCursorOptions {
            first_processed_batch: self.config.first_processed_batch,
            window_size: self.config.window_size,
            reset_state: self.config.reset,
        };
        let storage = if let Some(path) = self.config.db_path {
            VmPlaygroundStorageOptions::Rocksdb(path)
        } else {
            VmPlaygroundStorageOptions::Snapshots { shadow: false }
        };
        let (playground, tasks) = VmPlayground::new(
            connection_pool,
            dumps_object_store,
            self.config.fast_vm_mode,
            storage,
            self.zksync_network_id,
            cursor,
        )
        .await?;

        app_health
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
        (*self).run(stop_receiver.0).await
    }
}
