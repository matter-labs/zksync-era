use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::commitment::L1BatchCommitmentMode;

use super::http_client::HttpClient;
use crate::processor::{Locking, Processor};

pub(crate) struct ProofGenDataSubmitter {
    processor: Processor<Locking>,
    config: ProofDataHandlerConfig,
    client: HttpClient,
}

impl ProofGenDataSubmitter {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Core>,
        config: ProofDataHandlerConfig,
        commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        let processor = Processor::new(
            blob_store.clone(),
            pool.clone(),
            config.clone(),
            commitment_mode,
        );

        let Some(api_url) = config.gateway_api_url.clone() else {
            panic!("Gateway API URL should be set if running in prover cluster mode");
        };

        let client = HttpClient::new(api_url);
        Self {
            processor,
            config,
            client,
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("Starting proof generation data submitter");

        loop {
            if *stop_receiver.borrow() {
                tracing::info!(
                    "Stop signal received, proof generation data submitter is shutting down"
                );
                break;
            }

            let proof = self.client.fetch_proof().await?;

            if let Some((batch_number, proof)) = proof {
                self.processor
                    .save_proof(batch_number, proof)
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;

                self.client
                    .received_final_proof_request(batch_number)
                    .await?;
            } else {
                tracing::info!("No proof is ready yet");
            }

            tokio::time::sleep(self.config.proof_gen_data_submit_interval()).await;
        }

        Ok(())
    }
}