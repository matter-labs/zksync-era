use std::sync::Arc;

use zksync_config::configs::{fri_prover_gateway::ApiMode, ProofDataHandlerConfig};
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_proof_data_handler::ProofDataHandlerClient;
use zksync_types::{commitment::L1BatchCommitmentMode, L2ChainId};

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

/// Wiring layer for proof data handler server.
#[derive(Debug)]
pub struct ProofDataHandlerLayer {
    proof_data_handler_config: ProofDataHandlerConfig,
    commitment_mode: L1BatchCommitmentMode,
    l2_chain_id: L2ChainId,
    api_mode: ApiMode,
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
    pub task: ProofDataHandlerTask,
}

impl ProofDataHandlerLayer {
    pub fn new(
        proof_data_handler_config: ProofDataHandlerConfig,
        commitment_mode: L1BatchCommitmentMode,
        l2_chain_id: L2ChainId,
        api_mode: ApiMode,
    ) -> Self {
        Self {
            proof_data_handler_config,
            commitment_mode,
            l2_chain_id,
            api_mode,
        }
    }
}

#[async_trait::async_trait]
impl WiringLayer for ProofDataHandlerLayer {
    type Input = Input;
    type Output = Output;

    fn layer_name(&self) -> &'static str {
        "proof_data_handler_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let main_pool = input.master_pool.get().await?;
        let blob_store = input.object_store.0;

        let task = ProofDataHandlerTask {
            proof_data_handler_config: self.proof_data_handler_config,
            blob_store,
            main_pool,
            commitment_mode: self.commitment_mode,
            l2_chain_id: self.l2_chain_id,
            api_mode: self.api_mode,
        };

        Ok(Output { task })
    }
}

#[derive(Debug)]
pub struct ProofDataHandlerTask {
    proof_data_handler_config: ProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    main_pool: ConnectionPool<Core>,
    api_mode: ApiMode,
    commitment_mode: L1BatchCommitmentMode,
    l2_chain_id: L2ChainId,
}

#[async_trait::async_trait]
impl Task for ProofDataHandlerTask {
    fn id(&self) -> TaskId {
        "proof_data_handler".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let server_task = zksync_proof_data_handler::run_server(
            self.proof_data_handler_config.clone(),
            self.blob_store.clone(),
            self.main_pool.clone(),
            self.commitment_mode,
            self.l2_chain_id,
            self.api_mode.clone(),
            stop_receiver.clone().0,
        );

        if self.api_mode == ApiMode::Legacy {
            server_task.await
        } else {
            let client = ProofDataHandlerClient::new(
                self.blob_store,
                self.main_pool,
                self.proof_data_handler_config,
                self.commitment_mode,
                self.l2_chain_id,
            );

            let client_task = client.run(stop_receiver.0);

            tokio::select! {
                _ = server_task => {
                    tracing::info!("Proof data handler server stopped");
                }
                _ = client_task => {
                    tracing::info!("Proof data handler client stopped");
                }
            }

            Ok(())
        }
    }
}
