use reqwest::Client;
use secp256k1::{ecdsa::Signature, PublicKey, Secp256k1, SecretKey};
use serde::{de::DeserializeOwned, Serialize};
use url::Url;
use zksync_basic_types::H256;
use zksync_prover_interface::{
    api::{
        RegisterTeeAttestationRequest, RegisterTeeAttestationResponse, SubmitProofResponse,
        SubmitTeeProofRequest, TeeProofGenerationDataRequest, TeeProofGenerationDataResponse,
    },
    inputs::TeeVerifierInput,
    outputs::L1BatchTeeProofForL1,
};
use zksync_types::{tee_types::TeeType, L1BatchNumber};
pub(crate) struct ApiClient {
    api_base_url: Url,
    http_client: Client,
}

impl ApiClient {
    pub fn new(api_base_url: Url, http_client: Client) -> Self {
        ApiClient {
            api_base_url,
            http_client,
        }
    }

    async fn send_http_request<Req, Resp>(
        &self,
        request: Req,
        endpoint: Url,
    ) -> Result<Resp, reqwest::Error>
    where
        Req: Serialize,
        Resp: DeserializeOwned,
    {
        tracing::info!("Sending request to {}", endpoint);

        self.http_client
            .post(endpoint)
            .json(&request)
            .send()
            .await?
            .error_for_status()?
            .json::<Resp>()
            .await
    }

    pub async fn register_attestation(
        &self,
        attestation_quote_bytes: Vec<u8>,
        signing_key: &SecretKey,
    ) -> anyhow::Result<()> {
        let endpoint = self.api_base_url.join("/tee/register_attestation")?;
        let secp = Secp256k1::new();
        let request = RegisterTeeAttestationRequest {
            attestation: attestation_quote_bytes,
            pubkey: signing_key.public_key(&secp).serialize().to_vec(),
        };
        let response = self
            .send_http_request::<RegisterTeeAttestationRequest, RegisterTeeAttestationResponse>(
                request,
                endpoint.clone(),
            )
            .await?;
        match response {
            RegisterTeeAttestationResponse::Success => {
                tracing::info!("Attestation quote was successfully registered");
                Ok(())
            }
            RegisterTeeAttestationResponse::Error(error) => {
                let err_msg = format!("Registering attestation quote failed: {}", error);
                tracing::error!(err_msg);
                Err(anyhow::anyhow!(err_msg))
            }
        }
    }

    pub async fn get_job(&self) -> anyhow::Result<Option<Box<TeeVerifierInput>>> {
        let endpoint = self.api_base_url.join("/tee/proof_inputs")?;
        let request = TeeProofGenerationDataRequest {};
        let response = self
            .send_http_request::<TeeProofGenerationDataRequest, TeeProofGenerationDataResponse>(
                request,
                endpoint.clone(),
            )
            .await?;
        match response {
            TeeProofGenerationDataResponse::Success(tvi) => Ok(tvi),
            TeeProofGenerationDataResponse::Error(err) => {
                let err_msg = format!("Failed to get proof gen data: {:?}", err);
                tracing::error!(err_msg);
                Err(anyhow::anyhow!(err_msg))
            }
        }
    }

    pub async fn submit_proof(
        &self,
        batch_number: L1BatchNumber,
        signature: Signature,
        pubkey: &PublicKey,
        root_hash: H256,
        tee_type: TeeType,
    ) -> anyhow::Result<()> {
        let submit_proof_endpoint = self.api_base_url.join("/tee/submit_proofs")?;
        let mut endpoint = submit_proof_endpoint.clone();
        endpoint
            .path_segments_mut()
            .unwrap()
            .push(batch_number.to_string().as_str());
        let request = SubmitTeeProofRequest(Box::new(L1BatchTeeProofForL1 {
            signature: signature.serialize_compact().into(),
            pubkey: pubkey.serialize().into(),
            proof: root_hash.as_bytes().into(),
            tee_type,
        }));
        let response = self
            .send_http_request::<SubmitTeeProofRequest, SubmitProofResponse>(
                request,
                endpoint.clone(),
            )
            .await?;
        match response {
            SubmitProofResponse::Success => {
                tracing::info!("Proof was successfully submitted");
                Ok(())
            }
            SubmitProofResponse::Error(error) => {
                let err_msg = format!("Submission of the proof failed: {}", error);
                tracing::error!(err_msg);
                Err(anyhow::anyhow!(err_msg))
            }
        }
    }
}
