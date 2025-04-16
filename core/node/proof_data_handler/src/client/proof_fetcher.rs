use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::commitment::L1BatchCommitmentMode;

use super::http_client::HttpClient;
use crate::processor::Processor;

pub(crate) struct ProofFetcher {
    processor: Processor,
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

            if let Some(data) = self
                .processor
                .get_proof_generation_data()
                .await
                .map_err(|e| anyhow::anyhow!(e))?
            {
                self.client.send_proof_generation_data(data).await?;
            } else {
                tracing::info!("No proof generation data is ready yet");
            }

            tokio::time::sleep(self.config.proof_fetch_interval()).await;
        }

        Ok(())
    }
}