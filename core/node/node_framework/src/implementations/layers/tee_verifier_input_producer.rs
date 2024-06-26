use zksync_queued_job_processor::JobProcessor;
use zksync_tee_verifier_input_producer::TeeVerifierInputProducer;
use zksync_types::L2ChainId;

use crate::{
    implementations::resources::{
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for [`TeeVerifierInputProducer`].
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
///
/// ## Adds tasks
///
/// - `TeeVerifierInputProducer`
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
            .get_resource::<PoolResource<MasterPool>>()?
            .get()
            .await?;
        let object_store = context.get_resource::<ObjectStoreResource>()?;
        let tee =
            TeeVerifierInputProducer::new(pool_resource, object_store.0, self.l2_chain_id).await?;

        context.add_task(Box::new(tee));

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for TeeVerifierInputProducer {
    fn id(&self) -> TaskId {
        "tee_verifier_input_producer".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0, None).await
    }
}
