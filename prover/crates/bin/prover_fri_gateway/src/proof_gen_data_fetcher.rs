use std::sync::Arc;

use async_trait::async_trait;
use zksync_object_store::ObjectStore;
use zksync_prover_dal::{ConnectionPool, Prover, ProverDal};
use zksync_prover_interface::api::{
    ProofGenerationData, ProofGenerationDataRequest, ProofGenerationDataResponse,
};

use crate::{client::ProverApiClient, traits::PeriodicApi};

/// Poller structure that will periodically check the prover API for new proof generation data.
/// Fetched data is stored to the database/object store for further processing.
#[derive(Debug)]
pub struct ProofGenDataFetcher(ProverApiClient);

/// The path to the API endpoint that returns the next proof generation data.
const PROOF_GENERATION_DATA_PATH: &str = "/proof_generation_data";

impl ProofGenDataFetcher {
    pub(crate) fn new(
        blob_store: Arc<dyn ObjectStore>,
        base_url: String,
        pool: ConnectionPool<Prover>,
    ) -> Self {
        let api_url = format!("{base_url}{PROOF_GENERATION_DATA_PATH}");
        let inner = ProverApiClient::new(blob_store, pool, api_url);
        Self(inner)
    }
}

impl ProofGenDataFetcher {
    async fn save_proof_gen_data(&self, data: ProofGenerationData) {
        let store = &*self.0.blob_store;
        let witness_inputs = store
            .put(data.l1_batch_number, &data.witness_input_data)
            .await
            .expect("Failed to save proof generation data to GCS");
        let mut connection = self.0.pool.connection().await.unwrap();

        connection
            .fri_protocol_versions_dal()
            .save_prover_protocol_version(data.protocol_version, data.l1_verifier_config)
            .await;

        connection
            .fri_witness_generator_dal()
            .save_witness_inputs(data.l1_batch_number, &witness_inputs, data.protocol_version)
            .await;
    }
}

#[async_trait]
impl PeriodicApi for ProofGenDataFetcher {
    type JobId = ();
    type Request = ProofGenerationDataRequest;
    type Response = ProofGenerationDataResponse;

    const SERVICE_NAME: &'static str = "ProofGenDataFetcher";

    async fn get_next_request(&self) -> Option<(Self::JobId, ProofGenerationDataRequest)> {
        Some(((), ProofGenerationDataRequest {}))
    }

    async fn send_request(
        &self,
        _: (),
        request: ProofGenerationDataRequest,
    ) -> reqwest::Result<Self::Response> {
        self.0.send_http_request(request, &self.0.api_url).await
    }

    async fn handle_response(&self, _: (), response: Self::Response) {
        match response {
            ProofGenerationDataResponse::Success(Some(data)) => {
                tracing::info!("Received proof gen data for: {:?}", data.l1_batch_number);
                self.save_proof_gen_data(*data).await;
            }
            ProofGenerationDataResponse::Success(None) => {
                tracing::info!("There are currently no pending batches to be proven");
            }
            ProofGenerationDataResponse::Error(err) => {
                tracing::error!("Failed to get proof gen data: {:?}", err);
            }
        }
    }
}
