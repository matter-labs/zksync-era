use std::sync::Arc;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover};
use zksync_prover_interface::api::{ProofGenerationDataRequest, ProofGenerationDataResponse};

use crate::{client::ProverApiClient, proof_data_manager::ProofDataManager, traits::PeriodicApi};

/// Poller structure that will periodically check the prover API for new proof generation data.
/// Fetched data is stored to the database/object store for further processing.
#[derive(Debug)]
pub struct ProofGenDataFetcher {
    manager: ProofDataManager,
    client: ProverApiClient,
}

/// The path to the API endpoint that returns the next proof generation data.
const PROOF_GENERATION_DATA_PATH: &str = "/proof_generation_data";

impl ProofGenDataFetcher {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        base_url: String,
        pool: ConnectionPool<Prover>,
    ) -> Self {
        let api_url = format!("{base_url}{PROOF_GENERATION_DATA_PATH}");
        let client = ProverApiClient::new(api_url);
        let manager = ProofDataManager::new(blob_store.clone(), pool.clone());
        Self { manager, client }
    }
}

#[async_trait]
impl PeriodicApi for ProofGenDataFetcher {
    type JobId = ();
    type Request = ProofGenerationDataRequest;
    type Response = ProofGenerationDataResponse;

    const SERVICE_NAME: &'static str = "ProofGenDataFetcher";

    async fn get_next_request(
        &self,
    ) -> anyhow::Result<Option<(Self::JobId, ProofGenerationDataRequest)>> {
        Ok(Some(((), ProofGenerationDataRequest {})))
    }

    async fn send_request(
        &self,
        _: (),
        request: ProofGenerationDataRequest,
    ) -> reqwest::Result<Self::Response> {
        self.client
            .send_http_request(request, &self.client.api_url)
            .await
    }

    async fn handle_response(&self, _: (), response: Self::Response) -> anyhow::Result<()> {
        match response {
            ProofGenerationDataResponse::Success(Some(data)) => {
                tracing::info!("Received proof gen data for: {:?}", data.l1_batch_number);
                self.manager.save_proof_gen_data(*data).await?;
            }
            ProofGenerationDataResponse::Success(None) => {
                tracing::info!("There are currently no pending batches to be proven");
            }
            ProofGenerationDataResponse::Error(err) => {
                tracing::error!("Failed to get proof gen data: {:?}", err);
            }
        }

        Ok(())
    }
}
