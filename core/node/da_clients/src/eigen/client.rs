use std::str::FromStr;

use chrono::{DateTime, Utc};
use reqwest::Client;
use rust_eigenda_signers::signers::private_key::Signer;
use rust_eigenda_v2_client::{
    core::BlobKey,
    payload_disperser::{PayloadDisperser, PayloadDisperserConfig},
    utils::SecretUrl as SecretUrlV2,
};
use rust_eigenda_v2_common::{Payload, PayloadForm};
use serde_json::{json, Value};
use subxt_signer::ExposeSecret;
use url::Url;
use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
    DataAvailabilityClient,
};

use crate::utils::{to_non_retriable_da_error, to_retriable_da_error};

// The JSON RPC Specification defines for the Server error (Reserved for implementation-defined server-errors) the range of codes -32000 to -32099
const PROOF_NOT_FOUND_ERROR_CODE: i64 = -32001;

// We can't implement DataAvailabilityClient for an outside struct, so it is needed to defined this intermediate struct
#[derive(Debug, Clone)]
pub struct EigenDAClient {
    client: PayloadDisperser,
    sidecar_client: Client,
    sidecar_rpc: Option<String>,
}

impl EigenDAClient {
    pub async fn new(config: EigenConfig, secrets: EigenSecrets) -> anyhow::Result<Self> {
        let url = Url::from_str(
            config
                .eigenda_eth_rpc
                .ok_or(anyhow::anyhow!("Eigenda eth rpc url is not set"))?
                .expose_str(),
        )?;

        let private_key = secrets.private_key.0.expose_secret();

        let payload_disperser_config = PayloadDisperserConfig {
            polynomial_form: PayloadForm::Coeff,
            blob_version: config.blob_version,
            cert_verifier_router_address: config.cert_verifier_router_addr,
            eth_rpc_url: SecretUrlV2::new(url),
            disperser_rpc: config.disperser_rpc,
            use_secure_grpc_flag: true,
            operator_state_retriever_addr: config.operator_state_retriever_addr,
            registry_coordinator_addr: config.registry_coordinator_addr,
        };

        let private_key = private_key.parse()?;
        let signer = Signer::new(private_key);
        let client = PayloadDisperser::new(payload_disperser_config, signer).await?;

        Ok(Self {
            client,
            sidecar_client: Client::new(),
            sidecar_rpc: config.eigenda_sidecar_rpc,
        })
    }
}

impl EigenDAClient {
    async fn send_blob_key(&self, blob_key: String) -> anyhow::Result<()> {
        let body = json!({
            "jsonrpc": "2.0",
            "method": "generate_proof",
            "params": { "blob_id": blob_key },
            "id": 1
        });
        let response = self
            .sidecar_client
            .post(
                self.sidecar_rpc
                    .clone()
                    .ok_or(anyhow::anyhow!("Failed to get sidecar rpc"))?,
            )
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
            .post(
                self.sidecar_rpc
                    .clone()
                    .ok_or(anyhow::anyhow!("Failed to get sidecar rpc"))?,
            )
            .json(&body)
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("Failed to get proof"))?;

        let json_response: Value = response
            .json()
            .await
            .map_err(|_| anyhow::anyhow!("Failed to parse response"))?;

        if let Some(error) = json_response.get("error") {
            if let Some(error_code) = error.get("code") {
                if error_code.as_i64() != Some(PROOF_NOT_FOUND_ERROR_CODE) {
                    return Err(anyhow::anyhow!("Failed to get proof for {:?}", blob_key));
                }
            }
        }

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
impl DataAvailabilityClient for EigenDAClient {
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

        // Sidecar RPC being set means we are using EigenDA V2 Secure
        if self.sidecar_rpc.is_some() {
            // In V2Secure, we need to send the blob key to the sidecar for proof generation
            self.send_blob_key(blob_key.to_hex())
                .await
                .map_err(to_retriable_da_error)?;
        }
        Ok(DispatchResponse::from(blob_key.to_hex()))
    }

    async fn ensure_finality(
        &self,
        dispatch_request_id: String,
        _: DateTime<Utc>,
    ) -> Result<Option<FinalityResponse>, DAError> {
        let bytes = hex::decode(dispatch_request_id.clone()).map_err(|_| {
            to_non_retriable_da_error(anyhow::anyhow!(
                "Failed to decode blob id: {}",
                dispatch_request_id
            ))
        })?;
        let blob_key = BlobKey::from_bytes(bytes.try_into().map_err(|_| {
            to_non_retriable_da_error(anyhow::anyhow!(
                "Failed to convert bytes to a 32-byte array"
            ))
        })?);
        let eigenda_cert = self
            .client
            .get_cert(&blob_key)
            .await
            .map_err(to_retriable_da_error)?;
        if eigenda_cert.is_some() {
            Ok(Some(FinalityResponse {
                blob_id: dispatch_request_id,
            }))
        } else {
            Ok(None)
        }
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        let bytes = hex::decode(blob_id).map_err(|err| {
            to_non_retriable_da_error(anyhow::anyhow!(
                "Failed to decode blob id: {}: {}",
                blob_id,
                err
            ))
        })?;
        let blob_key = BlobKey::from_bytes(bytes.try_into().map_err(|_| {
            to_non_retriable_da_error(anyhow::anyhow!(
                "Failed to convert bytes to a 32-byte array"
            ))
        })?);
        let eigenda_cert = self
            .client
            .get_cert(&blob_key)
            .await
            .map_err(to_retriable_da_error)?;
        if let Some(eigenda_cert) = eigenda_cert {
            // Sidecar RPC being set means we are using EigenDA V2 Secure
            if self.sidecar_rpc.is_some() {
                if let Some(proof) = self
                    .get_proof(blob_id)
                    .await
                    .map_err(to_non_retriable_da_error)?
                {
                    Ok(Some(InclusionData { data: proof }))
                } else {
                    Ok(None)
                }
            } else {
                let inclusion_data = eigenda_cert.to_bytes().map_err(|_| {
                    to_non_retriable_da_error(anyhow::anyhow!(
                        "Failed to convert eigenda cert to bytes"
                    ))
                })?;
                Ok(Some(InclusionData {
                    data: inclusion_data,
                }))
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
        ClientType::Eigen
    }

    async fn balance(&self) -> Result<u64, DAError> {
        Ok(0) // TODO fetch from API when payments are enabled in Eigen (PE-305)
    }
}
