use zksync_basic_types::{commitment::L1BatchCommitmentMode, L2ChainId};
use zksync_config::configs::{
    external_proof_integration_api::ExternalProofIntegrationApiConfig, ProofDataHandlerConfig,
};
use zksync_dal::node::{PoolResource, ReplicaPool};
use zksync_node_framework::{
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};
use zksync_object_store::node::ObjectStoreResource;
use zksync_proof_data_handler::{Processor, Readonly};

use crate::Api;

/// Wiring layer for proof data handler server.
#[derive(Debug)]
pub struct ExternalProofIntegrationApiLayer {
    external_proof_integration_api_config: ExternalProofIntegrationApiConfig,
    proof_data_handler_config: ProofDataHandlerConfig,
    commitment_mode: L1BatchCommitmentMode,
    l2_chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub replica_pool: PoolResource<ReplicaPool>,
    pub object_store: ObjectStoreResource,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    pub task: Api,
}

impl ExternalProofIntegrationApiLayer {
    pub fn new(
        external_proof_integration_api_config: ExternalProofIntegrationApiConfig,
        proof_data_handler_config: ProofDataHandlerConfig,
        commitment_mode: L1BatchCommitmentMode,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            external_proof_integration_api_config,
            proof_data_handler_config,
            commitment_mode,
            l2_chain_id,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ExternalProofIntegrationApiLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "external_proof_integration_api_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let replica_pool = input.replica_pool.get().await.unwrap();
        let blob_store = input.object_store.0;

        let processor = Processor::<Readonly>::new(
            blob_store,
            replica_pool,
            self.proof_data_handler_config,
            self.commitment_mode,
            self.l2_chain_id,
        );
        let task = Api::new(
            processor,
            self.external_proof_integration_api_config.http_port,
        );

        Ok(Output { task })
    }
}

#[async_trait::async_trait]
impl Task for Api {
    fn id(&self) -> TaskId {
        "external_proof_integration_api".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver.0).await
    }
}
