use std::{sync::Arc, time::Duration};

use zksync_config::configs::eth_proof_manager::EthProofManagerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_proof_data_handler::{Locking, Processor};
use zksync_types::L2ChainId;

use crate::{
    client::EthProofManagerClient,
    types::{ProofRequestIdentifier, ProofRequestParams},
};

pub struct ProofRequestSubmitter {
    client: Box<dyn EthProofManagerClient>,
    connection_pool: ConnectionPool<Core>,
    blob_store: Arc<dyn ObjectStore>,
    config: EthProofManagerConfig,
    processor: Processor<Locking>,
}

impl ProofRequestSubmitter {
    pub fn new(
        client: Box<dyn EthProofManagerClient>,
        blob_store: Arc<dyn ObjectStore>,
        connection_pool: ConnectionPool<Core>,
        config: EthProofManagerConfig,
        proof_generation_timeout: Duration,
        l2_chain_id: L2ChainId,
    ) -> Self {
        let processor = Processor::<Locking>::new(
            blob_store.clone(),
            connection_pool.clone(),
            proof_generation_timeout,
            l2_chain_id,
            ProvingMode::ProvingNetwork,
        );
        Self {
            client,
            connection_pool,
            blob_store,
            config,
            processor,
        }
    }

    pub async fn loop_iteration(&self) -> anyhow::Result<()> {
        let batch_id = self.processor.lock_batch_for_proving_network().await?;

        if let Some(batch_id) = batch_id {
            let proof_generation_data = self
                .processor
                .proof_generation_data_for_existing_batch(batch_id)
                .await?;

            let url = self.blob_store.put(proof_generation_data).await?;

            let proof_request_identifier = ProofRequestIdentifier {
                chain_id: proof_generation_data.chain_id,
                block_number: proof_generation_data.l1_batch_number,
            };

            let proof_request_parameters = ProofRequestParams {
                protocol_major: proof_generation_data.protocol_version.major,
                protocol_minor: proof_generation_data.protocol_version.minor,
                protocol_patch: proof_generation_data.protocol_version.patch,
                proof_inputs_url: url,
                timeout_after: self.config.proof_request_timeout,
                max_reward: self.config.max_reward,
            };

            if let Err(e) = self
                .client
                .submit_proof_request(proof_request_identifier, proof_request_parameters)
                .await
            {
                tracing::error!(
                    "Failed to submit proof request for batch {}: {}",
                    batch_id,
                    e
                );
            } else {
                // todo: add a check to see if the proof request was accepted
            }
        }
    }
}
