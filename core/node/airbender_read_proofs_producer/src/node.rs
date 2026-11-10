use std::sync::Arc;

use zksync_config::configs::{AirbenderProofDataHandlerConfig, AirbenderReadProofsProducerConfig};
use zksync_dal::node::{MasterPool, PoolResource};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_object_store::ObjectStore;
use zksync_shared_resources::tree::TreeApiClient;

use crate::AirbenderReadProofsProducer;

/// Wiring layer for the Airbender read-proofs producer. It takes the handler config to work on
/// the same batches the handler can hand out.
#[derive(Debug)]
pub struct AirbenderReadProofsProducerLayer {
    config: AirbenderReadProofsProducerConfig,
    handler_config: AirbenderProofDataHandlerConfig,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    object_store: Arc<dyn ObjectStore>,
    tree_api_client: Arc<dyn TreeApiClient>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    task: AirbenderReadProofsProducer,
}

impl AirbenderReadProofsProducerLayer {
    pub fn new(
        config: AirbenderReadProofsProducerConfig,
        handler_config: AirbenderProofDataHandlerConfig,
    ) -> Self {
        Self {
            config,
            handler_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for AirbenderReadProofsProducerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "airbender_read_proofs_producer_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let task = AirbenderReadProofsProducer::new(
            input.master_pool.get().await?,
            input.object_store,
            input.tree_api_client,
            self.handler_config.first_processed_batch,
            self.handler_config.max_proving_attempts,
            self.config.poll_interval,
        );
        Ok(Output { task })
    }
}

#[async_trait::async_trait]
impl Task for AirbenderReadProofsProducer {
    fn id(&self) -> TaskId {
        "airbender_read_proofs_producer".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
