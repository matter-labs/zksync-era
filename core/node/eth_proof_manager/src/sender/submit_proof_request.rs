use std::{sync::Arc, time::Duration};

use zksync_config::configs::{
    eth_proof_manager::EthProofManagerConfig, proof_data_handler::ProvingMode,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_proof_data_handler::{Locking, Processor};
use zksync_types::{L1BatchId, L2ChainId};

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

            let url = self
                .blob_store
                .put(
                    (
                        L1BatchId::new(self.processor.chain_id(), batch_id),
                        proof_generation_data.protocol_version,
                    ),
                    &proof_generation_data,
                )
                .await?;

            let proof_request_identifier = ProofRequestIdentifier {
                chain_id: proof_generation_data.chain_id.as_u64(),
                block_number: proof_generation_data.l1_batch_number.0 as u64,
            };

            let proof_request_parameters = ProofRequestParams {
                protocol_major: 0,
                protocol_minor: proof_generation_data.protocol_version.minor as u32,
                protocol_patch: proof_generation_data.protocol_version.patch.0 as u32,
                proof_inputs_url: url,
                timeout_after: self.config.proof_request_timeout.as_secs() as u64,
                max_reward: self.config.max_reward,
            };

            match self
                .client
                .submit_proof_request(proof_request_identifier, proof_request_parameters)
                .await
            {
                Ok(tx_hash) => {
                    self.connection_pool
                        .connection()
                        .await?
                        .eth_proof_manager_dal()
                        .mark_batch_as_sent(batch_id, tx_hash)
                        .await?;
                    tracing::info!(
                        "Submitted proof request for batch {} with tx hash {}",
                        batch_id,
                        tx_hash
                    );
                    return Ok(());
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to submit proof request for batch {}, moving to prover cluster: {}",
                        batch_id,
                        e
                    );
                    self.connection_pool
                        .connection()
                        .await?
                        .eth_proof_manager_dal()
                        .fallback_certain_batch(batch_id)
                        .await?;
                }
            }
        } else {
            tracing::info!("No batches to submit proof request for");
        }

        Ok(())
    }
}
