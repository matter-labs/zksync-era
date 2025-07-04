use std::sync::Arc;

use zksync_config::configs::TeeProofDataHandlerConfig;
use zksync_dal::{
    node::{MasterPool, PoolResource},
    ConnectionPool, Core,
};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_object_store::{node::ObjectStoreResource, ObjectStore};
use zksync_types::L2ChainId;

/// Wiring layer for proof data handler server.
#[derive(Debug)]
pub struct TeeProofDataHandlerLayer {
    proof_data_handler_config: TeeProofDataHandlerConfig,
    l2_chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub master_pool: PoolResource<MasterPool>,
    pub object_store: ObjectStoreResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub task: TeeProofDataHandlerTask,
}

impl TeeProofDataHandlerLayer {
    pub fn new(
        proof_data_handler_config: TeeProofDataHandlerConfig,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            proof_data_handler_config,
            l2_chain_id,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for TeeProofDataHandlerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "tee_proof_data_handler_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let main_pool = input.master_pool.get().await?;
        let blob_store = input.object_store.0;

        let task = TeeProofDataHandlerTask {
            proof_data_handler_config: self.proof_data_handler_config,
            blob_store,
            main_pool,
            l2_chain_id: self.l2_chain_id,
        };

        Ok(Output { task })
    }
}

#[derive(Debug)]
pub struct TeeProofDataHandlerTask {
    proof_data_handler_config: TeeProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    main_pool: ConnectionPool<Core>,
    l2_chain_id: L2ChainId,
}

#[async_trait::async_trait]
impl Task for TeeProofDataHandlerTask {
    fn id(&self) -> TaskId {
        "tee_proof_data_handler".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        crate::run_server(
            self.proof_data_handler_config,
            self.blob_store,
            self.main_pool,
            self.l2_chain_id,
            stop_receiver.0,
        )
        .await
    }
}
