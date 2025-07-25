use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::{
    eth_proof_manager::EthProofManagerConfig, proof_data_handler::ProvingMode,
};
use zksync_dal::{ConnectionPool, Core, CoreDal};
use zksync_object_store::ObjectStore;
use zksync_proof_data_handler::{Locking, Processor};
use zksync_prover_interface::inputs::PublicWitnessInputData;
use zksync_types::{L1BatchId, L1BatchNumber, L2ChainId};

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
        l2_chain_id: L2ChainId,
    ) -> Self {
        let processor = Processor::<Locking>::new(
            blob_store.clone(),
            connection_pool.clone(),
            config.proof_generation_timeout,
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

    pub async fn run(&self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        loop {
            if *stop_receiver.borrow() {
                tracing::info!("Stop request received, eth proof sender is shutting down");
                return Ok(());
            }

            if let Err(e) = self.loop_iteration().await {
                tracing::error!("Error submitting proof request: {e}");
            }

            tracing::info!(
                "Sleeping for {} seconds",
                self.config.request_sending_interval.as_secs()
            );
            tokio::time::sleep(self.config.request_sending_interval).await;
        }
    }

    pub async fn loop_iteration(&self) -> anyhow::Result<()> {
        let batch_id = self.processor.lock_batch_for_proving_network().await?;
        if let Some(batch_id) = batch_id {
            match self.submit_request(batch_id).await {
                Ok(_) => {
                    self.connection_pool
                        .connection()
                        .await?
                        .eth_proof_manager_dal()
                        .fallback_batch(batch_id)
                        .await?;
                }
                Err(e) => {
                    tracing::error!(
                        "Failed to submit proof request for batch {}, moving to prover cluster, error: {}",
                        batch_id,
                        e
                    );
                    self.connection_pool
                        .connection()
                        .await?
                        .eth_proof_manager_dal()
                        .fallback_batch(batch_id)
                        .await?;
                }
            }
        } else {
            tracing::info!("No batches to submit proof request for");
        }

        Ok(())
    }

    pub async fn submit_request(&self, batch_id: L1BatchNumber) -> anyhow::Result<()> {
        let proof_generation_data = self
            .processor
            .proof_generation_data_for_existing_batch(batch_id)
            .await?;

        tracing::info!("Need to send proof request for batch {}", batch_id);

        let witness_input_data =
            PublicWitnessInputData::new(proof_generation_data.witness_input_data.clone());

        let url = self
            .blob_store
            .put(
                L1BatchId::new(self.processor.chain_id(), batch_id),
                &witness_input_data,
            )
            .await
            .map_err(|e| {
                anyhow::anyhow!("Failed to put proof generation data into blob store: {}", e)
            })?;

        self.connection_pool
            .connection()
            .await?
            .eth_proof_manager_dal()
            .insert_batch(batch_id, &url)
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
            timeout_after: self.config.proof_generation_timeout.as_secs(),
            max_reward: self.config.max_reward,
        };

        let tx_hash = self
            .client
            .submit_proof_request(proof_request_identifier, proof_request_parameters)
            .await
            .map_err(|e| {
                anyhow::anyhow!(
                    "Failed to submit proof request for batch {}, error: {}",
                    batch_id,
                    e
                )
            })?;

        self.connection_pool
            .connection()
            .await?
            .eth_proof_manager_dal()
            .mark_batch_as_sent(batch_id, tx_hash)
            .await?;

        tracing::info!(
            "Submitted proof request for batch {}, chain_id: {}, with tx hash {}",
            proof_generation_data.l1_batch_number,
            proof_generation_data.chain_id,
            tx_hash
        );
        Ok(())
    }
}
