use std::str::FromStr;

use reqwest::Client;
use rust_eigenda_v2_client::{
    core::BlobKey,
    payload_disperser::{PayloadDisperser, PayloadDisperserConfig},
    rust_eigenda_signers::signers::private_key::Signer,
    utils::SecretUrl,
};
use rust_eigenda_v2_common::{Payload, PayloadForm};
use serde_json::{json, Value};
use subxt_signer::ExposeSecret;
use url::Url;
use zksync_config::{
    configs::da_client::eigenv2m1::{EigenSecretsV2M1, PolynomialForm},
    EigenConfigV2M1,
};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
    DataAvailabilityClient,
};

use crate::utils::{to_non_retriable_da_error, to_retriable_da_error};

// We can't implement DataAvailabilityClient for an outside struct, so it is needed to defined this intermediate struct
#[derive(Debug, Clone)]
pub struct EigenDAClientV2M1 {
    client: PayloadDisperser,
    sidecar_client: Client,
    sidecar_rpc: String,
}

impl EigenDAClientV2M1 {
    pub async fn new(config: EigenConfigV2M1, secrets: EigenSecretsV2M1) -> anyhow::Result<Self> {
        let url = Url::from_str(
            config
                .eigenda_eth_rpc
                .clone()
                .ok_or(anyhow::anyhow!("Eigenda eth rpc url is not set"))?
                .expose_str(),
        )
        .map_err(|_| anyhow::anyhow!("Invalid eth rpc url"))?;

        let payload_form = match config.polynomial_form {
            PolynomialForm::Coeff => PayloadForm::Coeff,
            PolynomialForm::Eval => PayloadForm::Eval,
        };

        let payload_disperser_config = PayloadDisperserConfig {
            polynomial_form: payload_form,
            blob_version: config.blob_version,
            cert_verifier_address: config.cert_verifier_addr,
            eth_rpc_url: SecretUrl::new(url),
            disperser_rpc: config.disperser_rpc,
            use_secure_grpc_flag: config.authenticated,
        };

        let private_key = secrets
            .private_key
            .0
            .expose_secret()
            .parse()
            .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;
        let signer = Signer::new(private_key);
        let client = PayloadDisperser::new(payload_disperser_config, signer)
            .await
            .map_err(|e| anyhow::anyhow!("Eigen client Error: {:?}", e))?;

        Ok(Self {
            client,
            sidecar_client: Client::new(),
            sidecar_rpc: config.eigenda_sidecar_rpc,
        })
    }
}

impl EigenDAClientV2M1 {
    async fn send_blob_key(&self, blob_key: String) -> anyhow::Result<()> {
        let body = json!({
            "jsonrpc": "2.0",
            "method": "generate_proof",
            "params": { "blob_id": blob_key },
            "id": 1
        });
        let response = self
            .sidecar_client
            .post(&self.sidecar_rpc)
            .json(&body)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Failed to send blob key"))?;

        let json_response: Value = response
            .json()
            .await
            .map_err(|_| anyhow::anyhow!("Failed to parse response"))?;

        if json_response.get("error").is_some() {
            Err(anyhow::anyhow!("Failed to send blob key"))
        } else {
            Ok(())
        }
    }

    async fn get_proof(&self, blob_key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let body = json!({
            "jsonrpc": "2.0",
            "method": "get_proof",
            "params": { "blob_id": blob_key },
            "id": 1
        });
        let response = self
            .sidecar_client
            .post(&self.sidecar_rpc)
            .json(&body)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Failed to get proof"))?;

        let json_response: Value = response
            .json()
            .await
            .map_err(|_| anyhow::anyhow!("Failed to parse response"))?;

        if let Some(result) = json_response.get("result") {
            if let Some(proof) = result.as_str() {
                let proof =
                    hex::decode(proof).map_err(|_| anyhow::anyhow!("Failed to parse proof"))?;
                return Ok(Some(proof));
            }
        }

        Ok(None)
    }
}

#[async_trait::async_trait]
impl DataAvailabilityClient for EigenDAClientV2M1 {
    async fn dispatch_blob(
        &self,
        _: u32, // batch number
        data: Vec<u8>,
    ) -> Result<DispatchResponse, DAError> {
        let payload = Payload::new(data);
        let blob_key = self
            .client
            .send_payload(payload)
            .await
            .map_err(to_retriable_da_error)?;

        let blob_key_hex = blob_key.to_hex();

        self.send_blob_key(blob_key_hex.clone())
            .await
            .map_err(to_non_retriable_da_error)?;

        Ok(DispatchResponse::from(blob_key_hex))
    }

    async fn ensure_finality(
        &self,
        dispatch_request_id: String,
    ) -> Result<Option<FinalityResponse>, DAError> {
        // TODO: return a quick confirmation in `dispatch_blob` and await here
        Ok(Some(FinalityResponse {
            blob_id: dispatch_request_id,
        }))
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        let blob_key = BlobKey::from_hex(blob_id)
            .map_err(|_| anyhow::anyhow!("Failed to decode blob id: {}", blob_id))
            .map_err(to_non_retriable_da_error)?;
        let eigenda_cert = self
            .client
            .get_inclusion_data(&blob_key)
            .await
            .map_err(to_retriable_da_error)?;
        if eigenda_cert.is_some() {
            if let Some(proof) = self
                .get_proof(blob_id)
                .await
                .map_err(to_retriable_da_error)?
            {
                Ok(Some(InclusionData { data: proof }))
            } else {
                Ok(None)
            }
        } else {
            Ok(None)
        }
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        PayloadDisperser::<Signer>::blob_size_limit()
    }

    fn client_type(&self) -> ClientType {
        ClientType::EigenV2M1
    }

    async fn balance(&self) -> Result<u64, DAError> {
        Ok(0) // TODO fetch from API when payments are enabled in Eigen (PE-305)
    }
}
