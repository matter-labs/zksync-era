use serde::{de::DeserializeOwned, Serialize};
use zksync_prover_interface::{
    api::{NextProof, ProofGenerationData, SubmitProofRequest},
    outputs::L1BatchProofForL1,
};
use zksync_types::L1BatchNumber;

pub(crate) struct HttpClient {
    api_url: String,
    client: reqwest::Client,
}

const SEND_PROOF_GENERATION_DATA_ENDPOINT: &str = "/submit_proof_generation_data";
const FETCH_PROOF_ENDPOINT: &str = "/get_next_proof";
const RECEIVED_FINAL_PROOF_ENDPOINT: &str = "/save_successful_sent_proof";

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
    ) -> Result<(), reqwest::Error> {
        tracing::info!("Sending request to {}", SEND_PROOF_GENERATION_DATA_ENDPOINT);

        let endpoint = self.api_url.clone() + SEND_PROOF_GENERATION_DATA_ENDPOINT;

        self.send_http_request(data, &endpoint).await
    }

    pub(crate) async fn fetch_proof(
        &self,
    ) -> Result<Option<(L1BatchNumber, L1BatchProofForL1)>, reqwest::Error> {
        tracing::info!("Sending request to {}", FETCH_PROOF_ENDPOINT);

        let endpoint = self.api_url.clone() + FETCH_PROOF_ENDPOINT;

        let response: Option<NextProof> = self.send_http_request((), &endpoint).await?;

        if let Some(next_proof) = response {
            let l1_batch_number = next_proof.l1_batch_number;
            let proof = match next_proof.request {
                SubmitProofRequest::Proof(proof) => proof,
                SubmitProofRequest::SkippedProofGeneration => unreachable!(),
            };

            Ok(Some((l1_batch_number, (*proof).into())))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn received_final_proof_request(
        &self,
        l1_batch_number: L1BatchNumber,
    ) -> Result<(), reqwest::Error> {
        tracing::info!("Sending request to {}", RECEIVED_FINAL_PROOF_ENDPOINT);

        let endpoint = self.api_url.clone() + RECEIVED_FINAL_PROOF_ENDPOINT;

        self.send_http_request(l1_batch_number, &endpoint).await
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
