use reqwest::{Client, Response, StatusCode};
use serde::Serialize;
use url::Url;
use zksync_basic_types::L1BatchNumber;
use zksync_airbender_prover_interface::{
    api::{SubmitAirbenderProofRequest, AirbenderProofGenerationDataRequest},
    inputs::AirbenderVerifierInput,
};

use crate::{error::AirbenderProverError, metrics::METRICS};

/// Implementation of the API client for the proof data handler, run by
/// [`zksync_airbender_proof_data_handler::run_server`].
#[derive(Debug)]
pub(crate) struct AirbenderApiClient {
    api_base_url: Url,
    http_client: Client,
}

impl AirbenderApiClient {
    pub fn new(api_base_url: Url) -> Self {
        AirbenderApiClient {
            api_base_url,
            http_client: Client::new(),
        }
    }

    async fn post<Req, S>(&self, endpoint: S, request: Req) -> Result<Response, reqwest::Error>
    where
        Req: Serialize + std::fmt::Debug,
        S: AsRef<str>,
    {
        let url = self.api_base_url.join(endpoint.as_ref()).unwrap();

        tracing::trace!("Sending POST request to {}: {:?}", url, request);

        self.http_client
            .post(url)
            .json(&request)
            .send()
            .await?
            .error_for_status()
    }

    /// Fetches the next job for the Airbender prover to process.
    pub async fn get_job(
        &self,
    ) -> Result<Option<AirbenderVerifierInput>, AirbenderProverError> {
        let request = AirbenderProofGenerationDataRequest;
        let response = self.post("/airbender/proof_inputs", request).await?;
        match response.status() {
            StatusCode::OK => Ok(Some(response.json::<AirbenderVerifierInput>().await?)),
            StatusCode::NO_CONTENT => Ok(None),
            _ => response
                .json::<Option<AirbenderVerifierInput>>()
                .await
                .map_err(AirbenderProverError::Request),
        }
    }

    /// Submits the successfully verified proof to the Airbender prover interface API.
    pub async fn submit_proof(
        &self,
        batch_number: L1BatchNumber,
        proof: Vec<u8>,
    ) -> Result<(), AirbenderProverError> {
        let request = SubmitAirbenderProofRequest { proof };
        let observer = METRICS.proof_submitting_time.start();
        self.post(
            format!("/airbender/submit_proofs/{batch_number}").as_str(),
            request,
        )
        .await?;
        observer.observe();
        tracing::info!(
            "Proof submitted successfully for batch number {}",
            batch_number
        );
        Ok(())
    }
}
