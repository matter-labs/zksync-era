use zksync_config::configs::ProofDataHandlerConfig;
use zksync_proof_data_handler::{
    ProofDataProcessor, ProofGenerationDataProcessor, ProofGenerationDataSubmitter,
    RequestProcessor, RpcClient, TeeProofDataHandler,
};
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
    task: ProofDataHandlerTask,
}

impl ProofDataHandlerLayer {
    pub fn new(
        proof_data_handler_config: ProofDataHandlerConfig,
        commitment_mode: L1BatchCommitmentMode,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            proof_data_handler_config,
            commitment_mode,
            l2_chain_id,
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

        let processor = RequestProcessor::new(
            blob_store.clone(),
            main_pool.clone(),
            self.proof_data_handler_config.clone(),
            self.l2_chain_id,
        );

        let api = if self.proof_data_handler_config.tee_config.tee_support {
            Some(TeeProofDataHandler::new_with_tee_support(
                processor,
                self.proof_data_handler_config.http_port,
            ))
        } else {
            None
        };

        let processor =
            ProofDataProcessor::new(main_pool.clone(), blob_store, self.commitment_mode);
        let rpc_client = RpcClient::new(
            processor,
            self.proof_data_handler_config.api_url,
            self.l2_chain_id,
            self.proof_data_handler_config.api_poll_duration(),
            self.proof_data_handler_config.retry_connection_interval(),
        );

        let task = ProofDataHandlerTask::new(api, rpc_client);

        Ok(Output { task })
    }
}

#[derive(Debug)]
struct ProofDataHandlerTask {
    tee_api: Option<TeeProofDataHandler>,
    rpc_client: RpcClient,
}

impl ProofDataHandlerTask {
    pub fn new(tee_api: Option<TeeProofDataHandler>, rpc_client: RpcClient) -> Self {
        Self {
            tee_api,
            rpc_client,
        }
    }

    async fn run(self, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let rpc_client = self.rpc_client;

        if let Some(tee_api) = self.tee_api {
            tokio::select! {
                _ = tee_api.run(stop_receiver.0.clone()) => {
                    tracing::info!("Proof data handler API stopped");
                }
                _ = rpc_client.run(stop_receiver.0.clone()) => {
                    tracing::info!("Rpc client stopped");
                }
            }
        } else {
            rpc_client.run(stop_receiver.0.clone()).await?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl Task for ProofDataHandlerTask {
    fn id(&self) -> TaskId {
        "proof_data_handler".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver).await
    }
}
