use async_trait::async_trait;
use zksync_prover_interface::{
    api::{TeeProofGenerationDataRequest, TeeProofGenerationDataResponse},
    inputs::TeeVerifierInput,
};

use crate::api_data_fetcher::{PeriodicApi, PeriodicApiStruct};

#[async_trait]
impl PeriodicApi<TeeProofGenerationDataRequest> for PeriodicApiStruct {
    type JobId = ();
    type Response = TeeProofGenerationDataResponse;

    const SERVICE_NAME: &'static str = "TeeVerifierInputDataFetcher";

    async fn get_next_request(&self) -> Option<(Self::JobId, TeeProofGenerationDataRequest)> {
        Some(((), TeeProofGenerationDataRequest {}))
    }

    async fn send_request(
        &self,
        _: (),
        request: TeeProofGenerationDataRequest,
    ) -> reqwest::Result<Self::Response> {
        self.send_http_request(request, &self.api_url).await
    }

    async fn handle_response(&self, _: (), response: Self::Response) {
        match response {
            TeeProofGenerationDataResponse::Success(Some(data)) => {
                match *data {
                    TeeVerifierInput::V0 => {
                        tracing::info!("Received unsupported TEE verifier input");
                    }
                    TeeVerifierInput::V1(data) => {
                        tracing::info!(
                            "Received TEE verifier input data for: {:?}",
                            data.l1_batch_env.number
                        );
                        // TODO sign it and send back
                    }
                    _ => {
                        tracing::error!("Received unsupported TEE verifier input");
                    }
                }
            }
            TeeProofGenerationDataResponse::Success(None) => {
                tracing::info!("There are currently no pending batches to be proven");
            }
            TeeProofGenerationDataResponse::Error(err) => {
                tracing::error!("Failed to get proof gen data: {:?}", err);
            }
        }
    }
}
