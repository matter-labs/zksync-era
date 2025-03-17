use zksync_config::configs::TeeProofDataHandlerConfig;
use zksync_tee_proof_data_handler::{RequestProcessor, TeeProofDataHandler};
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
pub struct TeeProofDataHandlerLayer {
    proof_data_handler_config: TeeProofDataHandlerConfig,
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
    task: TeeProofDataHandlerTask,
}

impl TeeProofDataHandlerLayer {
    pub fn new(
        proof_data_handler_config: TeeProofDataHandlerConfig,
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
impl WiringLayer for TeeProofDataHandlerLayer {
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

        let api = TeeProofDataHandler::new(processor, self.proof_data_handler_config.http_port);

        let task = TeeProofDataHandlerTask::new(api);

        Ok(Output { task })
    }
}

#[derive(Debug)]
struct TeeProofDataHandlerTask {
    tee_api: TeeProofDataHandler,
}

impl TeeProofDataHandlerTask {
    pub fn new(tee_api: TeeProofDataHandler) -> Self {
        Self { tee_api }
    }

    async fn run(self, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        self.tee_api.run(stop_receiver.0.clone()).await
    }
}

#[async_trait::async_trait]
impl Task for TeeProofDataHandlerTask {
    fn id(&self) -> TaskId {
        "proof_data_handler".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        (*self).run(stop_receiver).await
    }
}
