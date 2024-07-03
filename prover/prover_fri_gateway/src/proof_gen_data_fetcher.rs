use async_trait::async_trait;
use zksync_prover_dal::ProverDal;
use zksync_prover_interface::api::{
    ProofGenerationData, ProofGenerationDataRequest, ProofGenerationDataResponse,
};

use crate::api_data_fetcher::{PeriodicApi, PeriodicApiStruct};

impl PeriodicApiStruct {
    async fn save_proof_gen_data(&self, data: ProofGenerationData) {
        let store = &*self.blob_store;
        let merkle_paths = store
            .put(data.l1_batch_number, &data.data.merkle_paths)
            .await
            .expect("Failed to save proof generation data to GCS");
        let witness_inputs = store
            .put(data.l1_batch_number, &data.data)
            .await
            .expect("Failed to save proof generation data to GCS");
        let mut connection = self.pool.connection().await.unwrap();

        connection
            .fri_protocol_versions_dal()
            .save_prover_protocol_version(data.protocol_version, data.l1_verifier_config)
            .await;

        connection
            .fri_witness_generator_dal()
            .save_witness_inputs(
                data.l1_batch_number,
                &merkle_paths,
                &witness_inputs,
                data.protocol_version,
                data.eip_4844_blobs,
            )
            .await;
    }
}

#[async_trait]
impl PeriodicApi<ProofGenerationDataRequest> for PeriodicApiStruct {
    type JobId = ();
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
        self.send_http_request(request, &self.api_url).await
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
