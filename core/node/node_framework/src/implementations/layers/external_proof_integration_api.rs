use zksync_config::configs::external_proof_integration_api::ExternalProofIntegrationApiConfig;
use zksync_external_proof_integration_api::{Api, Processor};
use zksync_types::commitment::L1BatchCommitmentMode;

use crate::{
    implementations::resources::{
        object_store::ObjectStoreResource,
        pools::{PoolResource, ReplicaPool},
    },
    service::StopReceiver,
    task::{Task, TaskId},
    wiring_layer::{WiringError, WiringLayer},
    FromContext, IntoContext,
};

/// Wiring layer for proof data handler server.
#[derive(Debug)]
pub struct ExternalProofIntegrationApiLayer {
    external_proof_integration_api_config: ExternalProofIntegrationApiConfig,
    commitment_mode: L1BatchCommitmentMode,
}

#[derive(Debug, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub replica_pool: PoolResource<ReplicaPool>,
    pub object_store: ObjectStoreResource,
}

#[derive(Debug, IntoContext)]
#[context(crate = crate)]
pub struct Output {
    #[context(task)]
    pub task: Api,
}

impl ExternalProofIntegrationApiLayer {
    pub fn new(
        external_proof_integration_api_config: ExternalProofIntegrationApiConfig,
        commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            external_proof_integration_api_config,
            commitment_mode,
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

        let processor = Processor::new(blob_store, replica_pool, self.commitment_mode);
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
