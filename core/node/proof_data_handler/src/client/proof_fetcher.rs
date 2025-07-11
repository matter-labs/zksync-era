use std::sync::Arc;

use tokio::sync::watch;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::{L1BatchId, L2ChainId};

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
        l2_chain_id: L2ChainId,
    ) -> Self {
        let processor = Processor::new(
            blob_store.clone(),
            pool.clone(),
            config.proof_generation_timeout,
            l2_chain_id,
            config.proving_mode.clone(),
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
                    "Stop request received, proof generation data submitter is shutting down"
                );
                break;
            }

            let Some(batch_to_fetch) = self.processor.get_oldest_not_proven_batch().await? else {
                tracing::info!("No batches to fetch proofs for");
                tokio::time::sleep(self.config.proof_fetch_interval).await;
                continue;
            };

            if let Err(e) = self
                .fetch_proof(L1BatchId::new(self.processor.chain_id(), batch_to_fetch))
                .await
            {
                tracing::error!("Request failed to get proof for batch {batch_to_fetch}: {e}");
            }

            if self.config.fetch_zero_chain_id_proofs {
                if let Err(e) = self
                    .fetch_proof(L1BatchId::new(L2ChainId::zero(), batch_to_fetch))
                    .await
                {
                    tracing::error!("Request failed to get proof for batch {batch_to_fetch}: {e}");
                }
            }

            tracing::info!(
                "No proof was fetched, sleeping for {:?}",
                self.config.proof_fetch_interval
            );
            tokio::time::sleep(self.config.proof_fetch_interval).await;
        }

        Ok(())
    }

    async fn fetch_proof(&self, l1_batch_id: L1BatchId) -> anyhow::Result<()> {
        match self.client.fetch_proof(l1_batch_id).await {
            Ok(Some(response)) => {
                if l1_batch_id.chain_id() != L2ChainId::zero()
                    && response.l1_batch_id.chain_id() != self.processor.chain_id()
                {
                    tracing::error!("Received proof for batch {l1_batch_id} with wrong chain id");
                    return Err(anyhow::anyhow!(
                        "Received proof for batch {l1_batch_id} with wrong chain id"
                    ));
                }

                tracing::info!("Received proof for batch {l1_batch_id}");

                self.processor
                    .save_proof(l1_batch_id, response.proof.into())
                    .await
                    .map_err(|e| anyhow::anyhow!(e))?;
                Ok(())
            }
            Ok(None) => {
                tracing::info!("No proof for batch {l1_batch_id} is ready yet");
                Ok(())
            }
            Err(e) => Err(anyhow::anyhow!(e)),
        }
    }
}
