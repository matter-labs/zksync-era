use std::sync::Arc;

use zksync_config::configs::external_proof_integration_api::ExternalProofIntegrationApiConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
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
    pub task: ExternalProofIntegrationApiTask,
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

        let task = ExternalProofIntegrationApiTask {
            external_proof_integration_api_config: self.external_proof_integration_api_config,
            blob_store,
            replica_pool,
            commitment_mode: self.commitment_mode,
        };

        Ok(Output { task })
    }
}

#[derive(Debug)]
pub struct ExternalProofIntegrationApiTask {
    external_proof_integration_api_config: ExternalProofIntegrationApiConfig,
    blob_store: Arc<dyn ObjectStore>,
    replica_pool: ConnectionPool<Core>,
    commitment_mode: L1BatchCommitmentMode,
}

#[async_trait::async_trait]
impl Task for ExternalProofIntegrationApiTask {
    fn id(&self) -> TaskId {
        "external_proof_integration_api".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        zksync_external_proof_integration_api::run_server(
            self.external_proof_integration_api_config,
            self.blob_store,
            self.replica_pool,
            self.commitment_mode,
            stop_receiver.0,
        )
        .await
    }
}
