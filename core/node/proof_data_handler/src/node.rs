use std::sync::Arc;

use zksync_config::configs::ProofDataHandlerConfig;
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
use zksync_object_store::ObjectStore;
use zksync_types::{basic_fri_types::ApiMode, L2ChainId};

use crate::ProofDataHandlerClient;

/// Wiring layer for proof data handler server.
#[derive(Debug)]
pub struct ProofDataHandlerLayer {
    proof_data_handler_config: ProofDataHandlerConfig,
    l2_chain_id: L2ChainId,
}

#[derive(Debug, FromContext)]
pub struct Input {
    master_pool: PoolResource<MasterPool>,
    object_store: Arc<dyn ObjectStore>,
}

#[derive(Debug, IntoContext)]
pub struct Output {
    #[context(task)]
    task: ProofDataHandlerTask,
}

impl ProofDataHandlerLayer {
    pub fn new(proof_data_handler_config: ProofDataHandlerConfig, l2_chain_id: L2ChainId) -> Self {
        Self {
            proof_data_handler_config,
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
        let blob_store = input.object_store;

        let task = ProofDataHandlerTask {
            config: self.proof_data_handler_config,
            blob_store,
            main_pool,
            l2_chain_id: self.l2_chain_id,
        };

        Ok(Output { task })
    }
}

#[derive(Debug)]
pub struct ProofDataHandlerTask {
    config: ProofDataHandlerConfig,
    blob_store: Arc<dyn ObjectStore>,
    main_pool: ConnectionPool<Core>,
    l2_chain_id: L2ChainId,
}

#[async_trait::async_trait]
impl Task for ProofDataHandlerTask {
    fn id(&self) -> TaskId {
        "proof_data_handler".into()
    }

    async fn run(self: Box<Self>, stop_receiver: StopReceiver) -> anyhow::Result<()> {
        let server_task = crate::run_server(
            self.config.clone(),
            self.blob_store.clone(),
            self.main_pool.clone(),
            self.l2_chain_id,
            stop_receiver.clone().0,
        );

        if self.config.api_mode == ApiMode::Legacy {
            server_task.await
        } else {
            let client = ProofDataHandlerClient::new(
                self.blob_store,
                self.main_pool,
                self.config,
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
