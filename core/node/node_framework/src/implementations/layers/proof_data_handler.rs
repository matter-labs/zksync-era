use std::sync::Arc;

use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::commitment::L1BatchCommitmentMode;

use crate::{
    implementations::resources::{
        object_store::ObjectStoreResource,
        pools::{MasterPool, PoolResource},
    },
    service::{ServiceContext, StopReceiver},
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for proof data handler server.
///
/// ## Requests resources
///
/// - `PoolResource<MasterPool>`
/// - `ObjectStoreResource`
///
/// ## Adds tasks
///
/// - `ProofDataHandlerTask`
#[derive(Debug)]
pub struct ProofDataHandlerLayer {
    proof_data_handler_config: ProofDataHandlerConfig,
    commitment_mode: L1BatchCommitmentMode,
}

impl ProofDataHandlerLayer {
    pub fn new(
        proof_data_handler_config: ProofDataHandlerConfig,
        commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            proof_data_handler_config,
            commitment_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ProofDataHandlerLayer {
    fn layer_name(&self) -> &'static str {
        "proof_data_handler_layer"
    }

    async fn wire(self: Box<Self>, mut context: ServiceContext<'_>) -> Result<(), WiringError> {
        let pool_resource = context.get_resource::<PoolResource<MasterPool>>()?;
        let main_pool = pool_resource.get().await.unwrap();

        let object_store = context.get_resource::<ObjectStoreResource>()?;

        context.add_task(ProofDataHandlerTask {
            proof_data_handler_config: self.proof_data_handler_config,
            blob_store: object_store.0,
            main_pool,
            commitment_mode: self.commitment_mode,
        });

        Ok(())
    }
}

#[derive(Debug)]
struct ProofDataHandlerTask {
    proof_data_handler_config: ProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    main_pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
}

#[async_trait::async_trait]
impl Task for ProofDataHandlerTask {
    fn id(&self) -> TaskId {
        "proof_data_handler".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        zksync_proof_data_handler::run_server(
            self.proof_data_handler_config,
            self.blob_store,
            self.main_pool,
            self.commitment_mode,
            stop_receiver.0,
        )
        .await
    }
}
