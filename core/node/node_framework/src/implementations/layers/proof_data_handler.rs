use std::sync::Arc;

use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_proof_data_handler::{
    ProofDataHandlerApi, ProofGenerationDataSubmitter, RequestProcessor,
};
use zksync_types::{api::Proof, commitment::L1BatchCommitmentMode, L2ChainId};

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
    pub task: ProofDataHandlerTask,
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
            blob_store,
            main_pool,
            self.proof_data_handler_config.clone(),
            self.commitment_mode,
            self.l2_chain_id,
        );

        let mut task =
            ProofDataHandlerApi::new(self.proof_data_handler_config.http_port, processor);
        if self.proof_data_handler_config.tee_config.tee_support {
            task = task.with_tee_support();
        }

        Ok(Output { task })
    }
}

#[derive(Debug)]
struct ProofDataHandlerTask {
    api: ProofDataHandlerApi,
    data_submitter: ProofGenerationDataSubmitter,
}

impl ProofDataHandlerTask {
    pub fn new(api: ProofDataHandlerApi, data_submitter: ProofGenerationDataSubmitter) -> Self {
        Self {
            api,
            data_submitter,
        }
    }

    async fn run(self, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let api = self.api;
        let data_submitter = self.data_submitter;

        tokio::select! {
            _ = api.run(stop_receiver.0.clone()) => {
                tracing::info!("Proof data handler API stopped");
            }
            _ = data_submitter.run(stop_receiver.0) => {
                tracing::info!("Proof data submitter stopped");
            }
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
