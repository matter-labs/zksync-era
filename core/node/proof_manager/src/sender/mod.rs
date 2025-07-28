use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::proof_manager::ProofManagerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::L2ChainId;

use crate::{
    client::BoxedProofManagerClient,
    sender::{
        submit_proof_request::ProofRequestSubmitter,
        submit_proof_validation::SubmitProofValidationSubmitter,
    },
};

mod submit_proof_request;
mod submit_proof_validation;

#[derive(Debug)]
pub struct ProofSender {
    client: Box<dyn BoxedProofManagerClient>,
    connection_pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    public_blob_store: Arc<dyn ObjectStore>,
    config: ProofManagerConfig,
    l2_chain_id: L2ChainId,
}

impl ProofSender {
    pub fn new(
        client: Box<dyn BoxedProofManagerClient>,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        public_blob_store: Arc<dyn ObjectStore>,
        config: ProofManagerConfig,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            client,
            connection_pool,
            blob_store,
            public_blob_store,
            config,
            l2_chain_id,
        }
    }

    pub async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("Starting eth proof sender");

        let proof_request_submitter = ProofRequestSubmitter::new(
            self.client.clone_boxed(),
            self.blob_store.clone(),
            self.public_blob_store.clone(),
            self.connection_pool.clone(),
            self.config.clone(),
            self.l2_chain_id,
        );
        let proof_validation_submitter = SubmitProofValidationSubmitter::new(
            self.client.clone_boxed(),
            self.connection_pool.clone(),
            self.l2_chain_id,
        );

        tokio::select! {
            _ = proof_request_submitter.run(stop_receiver.clone()) => {
                tracing::error!("Proof request submitter stopped");
            }
            _ = proof_validation_submitter.run(stop_receiver) => {
                tracing::error!("Proof validation result submitter stopped");
            }
        }

        Ok(())
    }
}
