use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::commitment::L1BatchCommitmentMode;

use super::http_client::HttpClient;
use crate::processor::{Locking, Processor};

pub(crate) struct ProofFetcher {
    processor: Processor<Locking>,
    config: ProofDataHandlerConfig,
    client: HttpClient,
}

impl ProofFetcher {
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
        tracing::info!("Starting proof fetcher");

        loop {
            if *stop_receiver.borrow() {
                tracing::info!(
                    "Stop signal received, proof generation data submitter is shutting down"
                );
                break;
            }

            let Some(batch_to_fetch) = self.processor.get_oldest_not_proven_batch().await? else {
                tracing::info!("No batches to fetch proofs for");
                tokio::time::sleep(self.config.proof_fetch_interval()).await;
                continue;
            };

            match self.client.fetch_proof(batch_to_fetch).await {
                Ok(Some(response)) => {
                    tracing::info!("Received proof for batch {batch_to_fetch}");

                    self.processor
                        .save_proof(response.l1_batch_number, response.proof.into())
                        .await
                        .map_err(|e| anyhow::anyhow!(e))?;
                    continue;
                }
                Ok(None) => {
                    tracing::info!("No proof is ready yet");
                }
                Err(e) => {
                    tracing::error!("Request failed to get proof for batch {batch_to_fetch}: {e}");
                }
            }

            tracing::info!(
                "No proof was fetched, sleeping for {:?}",
                self.config.proof_fetch_interval()
            );
            tokio::time::sleep(self.config.proof_fetch_interval()).await;
        }

        Ok(())
    }
}
