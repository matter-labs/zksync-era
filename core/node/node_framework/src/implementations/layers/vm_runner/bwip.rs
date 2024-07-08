use zksync_config::configs::vm_runner::BasicWitnessInputProducerConfig;
use zksync_types::L2ChainId;
use zksync_vm_runner::BasicWitnessInputProducer;

use crate::{
    implementations::resources::{
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct BasicWitnessInputProducerLayer {
    basic_witness_input_producer_config: BasicWitnessInputProducerConfig,
    zksync_network_id: L2ChainId,
}

impl BasicWitnessInputProducerLayer {
    pub fn new(
        basic_witness_input_producer_config: BasicWitnessInputProducerConfig,
        zksync_network_id: L2ChainId,
    ) -> Self {
        Self {
            basic_witness_input_producer_config,
            zksync_network_id,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for BasicWitnessInputProducerLayer {
    fn layer_name(&self) -> &'static str {
        "vm_runner_bwip"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let master_pool = context.get_resource::<PoolResource<MasterPool>>()?;
        let object_store = context.get_resource::<ObjectStoreResource>()?;

        let (basic_witness_input_producer, tasks) = BasicWitnessInputProducer::new(
            // One for `StorageSyncTask` which can hold a long-term connection in case it needs to
            // catch up cache.
            //
            // One for `ConcurrentOutputHandlerFactoryTask`/`VmRunner` as they need occasional access
            // to DB for querying last processed batch and last ready to be loaded batch.
            //
            // `window_size` connections for `BasicWitnessInputProducer`
            // as there can be multiple output handlers holding multi-second connections to process
            // BWIP data.
            master_pool
                .get_custom(self.basic_witness_input_producer_config.window_size + 2)
                .await?,
            object_store.0,
            self.basic_witness_input_producer_config.db_path,
            self.zksync_network_id,
            self.basic_witness_input_producer_config
                .first_processed_batch,
            self.basic_witness_input_producer_config.window_size,
        )
        .await?;

        context.add_task(tasks.loader_task);
        context.add_task(tasks.output_handler_factory_task);
        context.add_task(BasicWitnessInputProducerTask {
            basic_witness_input_producer,
        });
        Ok(())
    }
}

#[derive(Debug)]
struct BasicWitnessInputProducerTask {
    basic_witness_input_producer: BasicWitnessInputProducer,
}

#[async_trait::async_trait]
impl Task for BasicWitnessInputProducerTask {
    fn id(&self) -> TaskId {
        "vm_runner/bwip".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.basic_witness_input_producer
            .run(&stop_receiver.0)
            .await
    }
}
