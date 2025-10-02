use reqwest::{Client, Response, StatusCode};
use secp256k1::PublicKey;
use serde::Serialize;
use url::Url;
use zksync_basic_types::{tee_types::TeeType, L1BatchNumber, H256};
use zksync_tee_prover_interface::{
    api::{RegisterTeeAttestationRequest, SubmitTeeProofRequest, TeeProofGenerationDataRequest},
    inputs::TeeVerifierInput,
    outputs::L1BatchTeeProofForL1,
};

use crate::{error::TeeProverError, metrics::METRICS};

/// Implementation of the API client for the proof data handler, run by
/// [`zksync_tee_proof_data_handler::run_server`].
#[derive(Debug)]
pub(crate) struct TeeApiClient {
    api_base_url: Url,
    http_client: Client,
}

impl TeeApiClient {
    pub fn new(api_base_url: Url) -> Self {
        TeeApiClient {
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

    /// Registers the attestation quote with the TEE prover interface API, effectively proving that
    /// the private key associated with the given public key was used to sign the root hash within a
    /// trusted execution environment.
    pub async fn register_attestation(
        &self,
        attestation_quote_bytes: Vec<u8>,
        public_key: &PublicKey,
    ) -> Result<(), TeeProverError> {
        let request = RegisterTeeAttestationRequest {
            attestation: attestation_quote_bytes,
            pubkey: public_key.serialize().to_vec(),
        };
        self.post("/tee/register_attestation", request).await?;
        tracing::info!(
            "Attestation quote was successfully registered for the public key {}",
            public_key
        );
        Ok(())
    }

    /// Fetches the next job for the TEE prover to process, verifying and signing it if the
    /// verification is successful.
    pub async fn get_job(
        &self,
        tee_type: TeeType,
    ) -> Result<Option<TeeVerifierInput>, TeeProverError> {
        let request = TeeProofGenerationDataRequest { tee_type };
        let response = self.post("/tee/proof_inputs", request).await?;
        match response.status() {
            StatusCode::OK => Ok(Some(response.json::<TeeVerifierInput>().await?)),
            StatusCode::NO_CONTENT => Ok(None),
            _ => response
                .json::<Option<TeeVerifierInput>>()
                .await
                .map_err(TeeProverError::Request),
        }
    }

    /// Submits the successfully verified proof to the TEE prover interface API.
    pub async fn submit_proof(
        &self,
        batch_number: L1BatchNumber,
        signature: [u8; 65],
        pubkey: &PublicKey,
        root_hash: H256,
        tee_type: TeeType,
    ) -> Result<(), TeeProverError> {
        if tee_type == TeeType::None {
            tracing::info!("Not submitting proof from none TEE.");
            return Ok(());
        }
        let request = SubmitTeeProofRequest(Box::new(L1BatchTeeProofForL1 {
            signature: signature.into(),
            pubkey: pubkey.serialize().into(),
            proof: root_hash.as_bytes().into(),
            tee_type,
        }));
        let observer = METRICS.proof_submitting_time.start();
        self.post(
            format!("/tee/submit_proofs/{batch_number}").as_str(),
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
