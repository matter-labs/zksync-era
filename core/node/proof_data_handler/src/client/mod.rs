use std::sync::Arc;

use proof_fetcher::ProofFetcher;
use proof_gen_data_submitter::ProofGenDataSubmitter;
use tokio::sync::watch;
use zksync_config::configs::ProofDataHandlerConfig;
use zksync_dal::{ConnectionPool, Core};
use zksync_object_store::ObjectStore;
use zksync_types::commitment::L1BatchCommitmentMode;

mod http_client;
mod proof_fetcher;
mod proof_gen_data_submitter;

pub struct ProofDataHandlerClient {
    pub blob_store: Arc<dyn ObjectStore>,
    pub pool: ConnectionPool<Core>,
    pub config: ProofDataHandlerConfig,
    pub batch_commitment_mode: L1BatchCommitmentMode,
}

impl ProofDataHandlerClient {
    pub fn new(
        blob_store: Arc<dyn ObjectStore>,
        pool: ConnectionPool<Core>,
        config: ProofDataHandlerConfig,
        batch_commitment_mode: L1BatchCommitmentMode,
    ) -> Self {
        Self {
            blob_store,
            pool,
            config,
            batch_commitment_mode,
        }
    }

    pub async fn run(self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        tracing::info!("Starting proof data handler client");

        let proof_gen_data_submitter = ProofGenDataSubmitter::new(
            self.blob_store.clone(),
            self.pool.clone(),
            self.config.clone(),
            self.batch_commitment_mode,
        )
        .run(stop_receiver.clone());
        let proof_fetcher = ProofFetcher::new(
            self.blob_store,
            self.pool,
            self.config,
            self.batch_commitment_mode,
        )
        .run(stop_receiver);

        tracing::info!("Started proof data submitter and proof fetcher");

        tokio::select! {
            _ = proof_gen_data_submitter => {
                tracing::info!("Proof data submitter stopped");
            }
            _ = proof_fetcher => {
                tracing::info!("Proof fetcher stopped");
            }
        }

        Ok(())
    }
}
