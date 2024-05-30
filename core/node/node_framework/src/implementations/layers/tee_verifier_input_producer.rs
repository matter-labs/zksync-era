use zksync_queued_job_processor::JobProcessor;
use zksync_tee_verifier_input_producer::TeeVerifierInputProducer;
use zksync_types::L2ChainId;

use crate::{
    implementations::resources::{
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

#[derive(Debug)]
pub struct TeeVerifierInputProducerLayer {
    l2_chain_id: L2ChainId,
}

impl TeeVerifierInputProducerLayer {
    pub fn new(l2_chain_id: L2ChainId) -> Self {
        Self { l2_chain_id }
    }
}

#[async_trait::async_trait]
impl WiringLayer for TeeVerifierInputProducerLayer {
    fn layer_name(&self) -> &'static str {
        "tee_verifier_input_producer_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        // Get resources.
        let pool_resource = context
            .get_resource::<PoolResource<MasterPool>>()
            .await?
            .get()
            .await?;
        let object_store = context.get_resource::<ObjectStoreResource>().await?;
        let tee =
            TeeVerifierInputProducer::new(pool_resource, object_store.0, self.l2_chain_id).await?;

        context.add_task(Box::new(TeeVerifierInputProducerTask { tee }));

        Ok(())
    }
}

pub struct TeeVerifierInputProducerTask {
    tee: TeeVerifierInputProducer,
}

#[async_trait::async_trait]
impl Task for TeeVerifierInputProducerTask {
    fn name(&self) -> &'static str {
        "tee_verifier_input_producer"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.tee.run(stop_receiver.0, None).await
    }
}
