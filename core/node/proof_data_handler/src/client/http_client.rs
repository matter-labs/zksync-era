use serde::{de::DeserializeOwned, Serialize};
use zksync_prover_interface::api::{
    PollGeneratedProofsRequest, PollGeneratedProofsResponse, ProofGenerationData,
    SubmitProofGenerationDataResponse,
};
use zksync_types::L1BatchId;

pub(crate) struct HttpClient {
    api_url: String,
    client: reqwest::Client,
}

const SUBMIT_REQUEST_FOR_PROOFS_ENDPOINT: &str = "/submit_request_for_proofs";
const POLL_GENERATED_PROOFS_ENDPOINT: &str = "/poll_generated_proofs";

impl HttpClient {
    pub(crate) fn new(api_url: String) -> Self {
        Self {
            api_url,
            client: reqwest::Client::new(),
        }
    }

    pub(crate) async fn send_proof_generation_data(
        &self,
        data: ProofGenerationData,
    ) -> Result<SubmitProofGenerationDataResponse, reqwest::Error> {
        tracing::info!("Sending request to {}", SUBMIT_REQUEST_FOR_PROOFS_ENDPOINT);

        let endpoint = self.api_url.clone() + SUBMIT_REQUEST_FOR_PROOFS_ENDPOINT;

        self.send_http_request(data, &endpoint).await
    }

    pub(crate) async fn fetch_proof(
        &self,
        l1_batch_id: L1BatchId,
    ) -> Result<Option<PollGeneratedProofsResponse>, reqwest::Error> {
        tracing::info!("Sending request to {}", POLL_GENERATED_PROOFS_ENDPOINT);

        let endpoint = self.api_url.clone() + POLL_GENERATED_PROOFS_ENDPOINT;

        let request = PollGeneratedProofsRequest { l1_batch_id };

        self.send_http_request(request, &endpoint).await
    }

    async fn send_http_request<Req, Resp>(
        &self,
        request: Req,
        endpoint: &str,
    ) -> Result<Resp, reqwest::Error>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        tracing::info!("Sending request to {}", endpoint);

        self.client
            .post(endpoint)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<Resp>()
            .await
    }
}
