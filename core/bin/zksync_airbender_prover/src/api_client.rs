use reqwest::{Client, Response, StatusCode};
use secp256k1::PublicKey;
use serde::Serialize;
use url::Url;
use zksync_airbender_prover_interface::{
    api::{
        AirbenderProofGenerationDataRequest, RegisterAirbenderAttestationRequest,
        SubmitAirbenderProofRequest,
    },
    inputs::AirbenderVerifierInput,
    outputs::L1BatchAirbenderProofForL1,
};
use zksync_basic_types::{tee_types::TeeType, L1BatchNumber, H256};

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

    /// Registers the attestation quote with the Airbender prover interface API, effectively proving that
    /// the private key associated with the given public key was used to sign the root hash within a
    /// trusted execution environment.
    pub async fn register_attestation(
        &self,
        attestation_quote_bytes: Vec<u8>,
        public_key: &PublicKey,
    ) -> Result<(), AirbenderProverError> {
        let request = RegisterAirbenderAttestationRequest {
            attestation: attestation_quote_bytes,
            pubkey: public_key.serialize().to_vec(),
        };
        self.post("/airbender/register_attestation", request)
            .await?;
        tracing::info!(
            "Attestation quote was successfully registered for the public key {}",
            public_key
        );
        Ok(())
    }

    /// Fetches the next job for the Airbender prover to process, verifying and signing it if the
    /// verification is successful.
    pub async fn get_job(
        &self,
        tee_type: TeeType,
    ) -> Result<Option<AirbenderVerifierInput>, AirbenderProverError> {
        let request = AirbenderProofGenerationDataRequest { tee_type };
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
        signature: [u8; 65],
        pubkey: &PublicKey,
        root_hash: H256,
        tee_type: TeeType,
    ) -> Result<(), AirbenderProverError> {
        if tee_type == TeeType::None {
            tracing::info!("Not submitting proof from none TEE.");
            return Ok(());
        }
        let request = SubmitAirbenderProofRequest(Box::new(L1BatchAirbenderProofForL1 {
            signature: signature.into(),
            pubkey: pubkey.serialize().into(),
            proof: root_hash.as_bytes().into(),
            tee_type,
        }));
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
