use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::L2ChainId;

use crate::{
    client::EthProofManagerClient,
    sender::{
        submit_proof_request::ProofRequestSubmitter,
        submit_proof_validation::SubmitProofValidationSubmitter,
    },
};

mod submit_proof_request;
mod submit_proof_validation;

#[derive(Debug)]
pub struct EthProofSender {
    client: Box<dyn EthProofManagerClient>,
    connection_pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    config: EthProofManagerConfig,
    l2_chain_id: L2ChainId,
}

impl EthProofSender {
    pub fn new(
        client: Box<dyn EthProofManagerClient>,
        connection_pool: ConnectionPool<Core>,
        blob_store: Arc<dyn ObjectStore>,
        config: EthProofManagerConfig,
        l2_chain_id: L2ChainId,
    ) -> Self {
        Self {
            client,
            connection_pool,
            blob_store,
            config,
            l2_chain_id,
        }
    }

    pub async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("Starting eth proof sender");

        let proof_request_submitter = ProofRequestSubmitter::new(
            self.client.clone_boxed(),
            self.blob_store.clone(),
            self.connection_pool.clone(),
            self.config.clone(),
            self.l2_chain_id,
        );
        let proof_validation_submitter = SubmitProofValidationSubmitter::new(
            self.client.clone_boxed(),
            self.connection_pool.clone(),
            self.l2_chain_id,
        );

        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth proof sender is shutting down");
                break;
            }

            if let Err(e) = proof_request_submitter.loop_iteration().await {
                tracing::error!("Error submitting proof request: {e}");
            }

            if let Err(e) = proof_validation_submitter.loop_iteration().await {
                tracing::error!("Error submitting proof validation result: {e}");
            }

            tracing::info!(
                "Sleeping for {} seconds",
                self.config.request_sending_interval.as_secs()
            );
            tokio::time::sleep(self.config.request_sending_interval).await;
        }

        Ok(())
    }
}
