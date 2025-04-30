use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::{commitment::L1BatchCommitmentMode, L2ChainId};

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
        l2_chain_id: L2ChainId,
    ) -> Self {
        let processor = Processor::new(
            blob_store.clone(),
            pool.clone(),
            config.clone(),
            commitment_mode,
            l2_chain_id,
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
        tracing::info!("Starting proof gen data submitter");

        loop {
            if *stop_receiver.borrow() {
                tracing::info!(
                    "Stop signal received, proof generation data submitter is shutting down"
                );
                break;
            }

            if let Some(data) = self
                .processor
                .get_proof_generation_data()
                .await
                .map_err(|e| anyhow::anyhow!(e))?
            {
                match self.client.send_proof_generation_data(data.clone()).await {
                    Ok(_) => {
                        tracing::info!(
                            "Proof generation data sent successfully for batch {}",
                            data.l1_batch_number
                        );
                        continue;
                    }
                    Err(e) => {
                        self.processor.unlock_batch(data.l1_batch_number).await?;
                        tracing::error!(
                            "Failed to send proof generation data for batch {}: {e}",
                            data.l1_batch_number
                        );
                    }
                }
            } else {
                tracing::info!("No proof generation data is ready yet");
            }

            tracing::info!(
                "No proof generation was sent, sleeping for {:?}",
                self.config.proof_gen_data_submit_interval()
            );

            tokio::time::sleep(self.config.proof_gen_data_submit_interval()).await;
        }

        Ok(())
    }
}
