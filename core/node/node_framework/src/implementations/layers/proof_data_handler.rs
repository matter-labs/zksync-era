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
    ) -> Self {
        Self {
            proof_data_handler_config,
            commitment_mode,
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
        let main_pool = input.master_pool.get().await.unwrap();
        let blob_store = input.object_store.0;

        let task = ProofDataHandlerTask {
            proof_data_handler_config: self.proof_data_handler_config,
            blob_store,
            main_pool,
            commitment_mode: self.commitment_mode,
        };

        Ok(Output { task })
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
