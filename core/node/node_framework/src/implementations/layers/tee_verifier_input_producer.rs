use zksync_queued_job_processor::JobProcessor;
use zksync_tee_verifier_input_producer::TeeVerifierInputProducer;
use zksync_types::L2ChainId;

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

/// Wiring layer for [`TeeVerifierInputProducer`].
#[derive(Debug)]
pub struct TeeVerifierInputProducerLayer {
    tee_verifier_input_producer_config: TeeVerifierInputProducerConfig,
}

impl TeeVerifierInputProducerLayer {
    pub fn new(tee_verifier_input_producer_config: TeeVerifierInputProducerConfig) -> Self {
        Self {
            tee_verifier_input_producer_config,
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
    pub task: TeeVerifierInputProducer,
}

#[async_trait::async_trait]
impl WiringLayer for TeeVerifierInputProducerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "tee_verifier_input_producer_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let pool = input.master_pool.get().await?;
        let ObjectStoreResource(object_store) = input.object_store;
        let task = TeeVerifierInputProducer::new(
            pool,
            object_store,
            self.tee_verifier_input_producer_config,
        )
        .await?;

        Ok(Output { task })
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
