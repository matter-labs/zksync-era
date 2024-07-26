use zksync_config::configs::vm_runner::BasicWitnessInputProducerConfig;
use zksync_state_keeper::MainBatchExecutor;
use zksync_types::L2ChainId;
use zksync_vm_runner::{
    BasicWitnessInputProducer, BasicWitnessInputProducerIo, ConcurrentOutputHandlerFactoryTask,
    StorageSyncTask,
};

use crate::{
    implementations::resources::{
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

#[derive(Debug)]
pub struct BasicWitnessInputProducerLayer {
    config: BasicWitnessInputProducerConfig,
    zksync_network_id: L2ChainId,
}

impl BasicWitnessInputProducerLayer {
    pub fn new(
        basic_witness_input_producer_config: BasicWitnessInputProducerConfig,
        zksync_network_id: L2ChainId,
    ) -> Self {
        Self {
            config: basic_witness_input_producer_config,
            zksync_network_id,
        }
    }
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub object_store: ObjectStoreResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub output_handler_factory_task:
        ConcurrentOutputHandlerFactoryTask<BasicWitnessInputProducerIo>,
    #[context(task)]
    pub loader_task: StorageSyncTask<BasicWitnessInputProducerIo>,
    #[context(task)]
    pub basic_witness_input_producer: BasicWitnessInputProducer,
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

        // One for `StorageSyncTask` which can hold a long-term connection in case it needs to
        // catch up cache.
        //
        // One for `ConcurrentOutputHandlerFactoryTask`/`VmRunner` as they need occasional access
        // to DB for querying last processed batch and last ready to be loaded batch.
        //
        // `window_size` connections for `BasicWitnessInputProducer`
        // as there can be multiple output handlers holding multi-second connections to process
        // BWIP data.
        let connection_pool = master_pool.get_custom(self.config.window_size + 2).await?;

        // We don't get the executor from the context because it would contain state keeper-specific settings.
        let mut batch_executor = Box::new(MainBatchExecutor::new(false, false));
        batch_executor.set_fast_vm_mode(self.config.experimental_vm.fast_vm_mode);

        let (basic_witness_input_producer, tasks) = BasicWitnessInputProducer::new(
            connection_pool,
            object_store.0,
            batch_executor,
            self.config.db_path,
            self.zksync_network_id,
            self.config.first_processed_batch,
            self.config.window_size,
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
