use std::sync::Arc;

use zksync_config::configs::vm_runner::BasicWitnessInputProducerConfig;
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_object_store::ObjectStore;
use zksync_types::L2ChainId;
use zksync_vm_executor::batch::MainBatchExecutorFactory;

use crate::{
    impls::{BasicWitnessInputProducer, BasicWitnessInputProducerIo},
    ConcurrentOutputHandlerFactoryTask, StorageSyncTask,
};

/// Wiring layer for the BWIP component.
#[derive(Debug)]
pub struct BasicWitnessInputProducerLayer {
    config: BasicWitnessInputProducerConfig,
    zksync_network_id: L2ChainId,
}

impl BasicWitnessInputProducerLayer {
    /// Creates a layer with the provided config.
    pub fn new(config: BasicWitnessInputProducerConfig, zksync_network_id: L2ChainId) -> Self {
        Self {
            config,
            zksync_network_id,
        }
    }
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    object_store: Arc<dyn ObjectStore>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    output_handler_factory_task: ConcurrentOutputHandlerFactoryTask<BasicWitnessInputProducerIo>,
    #[context(task)]
    loader_task: StorageSyncTask<BasicWitnessInputProducerIo>,
    #[context(task)]
    basic_witness_input_producer: BasicWitnessInputProducer,
}

#[async_trait::async_trait]
impl WiringLayer for BasicWitnessInputProducerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "vm_runner_bwip"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let Input {
            master_pool,
            object_store,
        } = input;

        // - 1 connection for `StorageSyncTask` which can hold a long-term connection in case it needs to
        //   catch up cache.
        // - 1 connection for `ConcurrentOutputHandlerFactoryTask` / `VmRunner` as they need occasional access
        //   to DB for querying last processed batch and last ready to be loaded batch.
        // - `window_size` connections for `BasicWitnessInputProducer`
        //   as there can be multiple output handlers holding multi-second connections to process
        //   BWIP data.
        let connection_pool = master_pool
            .get_custom(self.config.window_size.get() + 2)
            .await?;

        // We don't get the executor from the context because it would contain state keeper-specific settings.
        let batch_executor = MainBatchExecutorFactory::<()>::new(false);

        let (basic_witness_input_producer, tasks) = BasicWitnessInputProducer::new(
            connection_pool,
            object_store,
            Box::new(batch_executor),
            self.config.db_path,
            self.zksync_network_id,
            self.config.first_processed_batch,
            self.config.window_size.get(),
        )
        .await?;

        Ok(Output {
            output_handler_factory_task: tasks.output_handler_factory_task,
            loader_task: tasks.loader_task,
            basic_witness_input_producer,
        })
    }
}

#[async_trait::async_trait]
impl Task for BasicWitnessInputProducer {
    fn id(&self) -> TaskId {
        "vm_runner/bwip".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(&stop_receiver.0).await
    }
}
