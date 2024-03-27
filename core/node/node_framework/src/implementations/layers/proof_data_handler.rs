use std::sync::Arc;

use zksync_config::configs::ProofDataHandlerConfig;
use zksync_core::proof_data_handler;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;

use crate::{
    implementations::resources::{object_store::ObjectStoreResource, pools::MasterPoolResource},
    service::{ServiceContext, StopReceiver},
    task::Task,
    wiring_layer::{WiringError, WiringLayer},
};

/// Builder for a proof data handler.
///
/// ## Effects
///
/// - Resolves `MasterPoolResource`.
/// - Resolves `ObjectStoreResource`.
/// - Adds `proof_data_handler` to the node.
#[derive(Debug)]
pub struct ProofDataHandlerLayer {
    proof_data_handler_config: ProofDataHandlerConfig,
}

impl ProofDataHandlerLayer {
    pub fn new(proof_data_handler_config: ProofDataHandlerConfig) -> Self {
        Self {
            proof_data_handler_config,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ProofDataHandlerLayer {
    fn layer_name(&self) -> &'static str {
        "proof_data_handler_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool_resource = context.get_resource::<MasterPoolResource>().await?;
        let main_pool = pool_resource.get().await.unwrap();

        let object_store = context.get_resource::<ObjectStoreResource>().await?;

        context.add_task(Box::new(ProofDataHandlerTask {
            proof_data_handler_config: self.proof_data_handler_config,
            blob_store: object_store.0,
            main_pool,
        }));

        Ok(())
    }
}

#[derive(Debug)]
struct ProofDataHandlerTask {
    proof_data_handler_config: ProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    main_pool: ConnectionPool<Core>,
}

#[async_trait::async_trait]
impl Task for ProofDataHandlerTask {
    fn name(&self) -> &'static str {
        "proof_data_handler"
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        proof_data_handler::run_server(
            self.proof_data_handler_config,
            self.blob_store,
            self.main_pool,
            stop_receiver.0,
        )
        .await
    }
}
