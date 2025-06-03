use std::{str::FromStr, sync::Arc};

use chrono::{DateTime, Utc};
use rust_eigenda_client::{
    client::BlobProvider,
    config::SrsPointsSource,
    rust_eigenda_signers::{signers::private_key::Signer, SecretKey},
    EigenClient,
};
use rust_eigenda_v2_client::{
    core::BlobKey,
    payload_disperser::{PayloadDisperser, PayloadDisperserConfig},
    utils::SecretUrl as SecretUrlV2,
};
use rust_eigenda_v2_common::{Payload, PayloadForm};
use subxt_signer::ExposeSecret;
use url::Url;
use zksync_config::{
    configs::da_client::eigenda::{EigenDASecrets, PointsSource, PolynomialForm, Version},
    EigenDAConfig,
};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
    DataAvailabilityClient,
};

use crate::utils::{to_non_retriable_da_error, to_retriable_da_error};

#[derive(Debug, Clone)]
enum InnerClient {
    V1(EigenClient),
    V2(PayloadDisperser),
}

// We can't implement DataAvailabilityClient for an outside struct, so it is needed to defined this intermediate struct
#[derive(Debug, Clone)]
pub struct EigenDAClient {
    client: InnerClient,
}

impl EigenDAClient {
    pub async fn new(
        config: EigenDAConfig,
        secrets: EigenDASecrets,
        blob_provider: Arc<dyn BlobProvider>,
    ) -> anyhow::Result<Self> {
        let url = Url::from_str(
            config
                .eigenda_eth_rpc
                .ok_or(anyhow::anyhow!("Eigenda eth rpc url is not set"))?
                .expose_str(),
        )
        .map_err(|_| anyhow::anyhow!("Invalid eth rpc url"))?;

        let private_key = secrets.private_key.0.expose_secret();

        let client = match config.version {
            Version::V1 => {
                let eth_rpc_url = rust_eigenda_client::config::SecretUrl::new(url);

                let srs_points_source = match config.points {
                    PointsSource::Path { path } => SrsPointsSource::Path(path),
                    PointsSource::Url { g1_url, g2_url } => SrsPointsSource::Url((g1_url, g2_url)),
                };

                let eigen_config = rust_eigenda_client::config::EigenConfig::new(
                    config.disperser_rpc,
                    eth_rpc_url,
                    config.settlement_layer_confirmation_depth,
                    config.eigenda_svc_manager_address,
                    config.wait_for_finalization,
                    config.authenticated,
                    srs_points_source,
                    config.custom_quorum_numbers,
                )?;
                let signer = Signer::new(
                    SecretKey::from_str(secrets.private_key.0.expose_secret())
                        .map_err(|_| anyhow::anyhow!("Invalid private key"))?,
                );

                let client = EigenClient::new(eigen_config, signer, blob_provider)
                    .await
                    .map_err(|e| anyhow::anyhow!("Eigen client Error: {:?}", e))?;
                InnerClient::V1(client)
            }
            Version::V2 => {
                let payload_form = match config.polynomial_form {
                    PolynomialForm::Coeff => PayloadForm::Coeff,
                    PolynomialForm::Eval => PayloadForm::Eval,
                };

                let payload_disperser_config = PayloadDisperserConfig {
                    polynomial_form: payload_form,
                    blob_version: config.blob_version,
                    cert_verifier_address: config.cert_verifier_addr,
                    eth_rpc_url: SecretUrlV2::new(url),
                    disperser_rpc: config.disperser_rpc,
                    use_secure_grpc_flag: config.authenticated,
                };

                let private_key = private_key
                    .parse()
                    .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;
                let signer = Signer::new(private_key);
                let client = PayloadDisperser::new(payload_disperser_config, signer)
                    .await
                    .map_err(|e| anyhow::anyhow!("EigenDA client Error: {:?}", e))?;
                InnerClient::V2(client)
            }
        };

        Ok(Self { client })
    }
}

#[async_trait::async_trait]
impl DataAvailabilityClient for EigenDAClient {
    async fn dispatch_blob(
        &self,
        _: u32, // batch number
        data: Vec<u8>,
    ) -> Result<DispatchResponse, DAError> {
        match &self.client {
            InnerClient::V1(client) => {
                let blob_id = client
                    .dispatch_blob(data)
                    .await
                    .map_err(to_retriable_da_error)?;

                Ok(DispatchResponse::from(blob_id))
            }
            InnerClient::V2(client) => {
                let payload = Payload::new(data);
                let blob_key = client
                    .send_payload(payload)
                    .await
                    .map_err(to_retriable_da_error)?;

                Ok(DispatchResponse::from(blob_key.to_hex()))
            }
        }
    }

    async fn ensure_finality(
        &self,
        dispatch_request_id: String,
        _: DateTime<Utc>,
    ) -> Result<Option<FinalityResponse>, DAError> {
        match &self.client {
            InnerClient::V1(client) => {
                let finalized = client
                    .check_finality(&dispatch_request_id)
                    .await
                    .map_err(to_retriable_da_error)?;
                if finalized {
                    Ok(Some(FinalityResponse {
                        blob_id: dispatch_request_id,
                    }))
                } else {
                    Ok(None)
                }
            }
            InnerClient::V2(client) => {
                let bytes = hex::decode(dispatch_request_id.clone())
                    .map_err(|_| {
                        anyhow::anyhow!("Failed to decode blob id: {}", dispatch_request_id)
                    })
                    .map_err(to_non_retriable_da_error)?;
                let blob_key = BlobKey::from_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("Failed to convert bytes to a 32-byte array"))
                        .map_err(to_non_retriable_da_error)?,
                );
                let eigenda_cert = client
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
        }
    }

    async fn get_inclusion_data(&self, blob_id: &str) -> Result<Option<InclusionData>, DAError> {
        match &self.client {
            InnerClient::V1(client) => {
                let inclusion_data = client
                    .get_inclusion_data(blob_id)
                    .await
                    .map_err(to_retriable_da_error)?;
                if let Some(inclusion_data) = inclusion_data {
                    Ok(Some(InclusionData {
                        data: inclusion_data,
                    }))
                } else {
                    Ok(None)
                }
            }
            InnerClient::V2(client) => {
                let bytes = hex::decode(blob_id)
                    .map_err(|_| anyhow::anyhow!("Failed to decode blob id: {}", blob_id))
                    .map_err(to_non_retriable_da_error)?;
                let blob_key = BlobKey::from_bytes(
                    bytes
                        .try_into()
                        .map_err(|_| anyhow::anyhow!("Failed to convert bytes to a 32-byte array"))
                        .map_err(to_non_retriable_da_error)?,
                );
                let eigenda_cert = client
                    .get_cert(&blob_key)
                    .await
                    .map_err(to_retriable_da_error)?;
                if let Some(eigenda_cert) = eigenda_cert {
                    let inclusion_data = eigenda_cert
                        .to_bytes()
                        .map_err(|_| anyhow::anyhow!("Failed to convert eigenda cert to bytes"))
                        .map_err(to_non_retriable_da_error)?;
                    Ok(Some(InclusionData {
                        data: inclusion_data,
                    }))
                } else {
                    Ok(None)
                }
            }
        }
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        match &self.client {
            InnerClient::V1(client) => client.blob_size_limit(),
            InnerClient::V2(_) => PayloadDisperser::<Signer>::blob_size_limit(),
        }
    }

    fn client_type(&self) -> ClientType {
        ClientType::EigenDA
    }

    async fn balance(&self) -> Result<u64, DAError> {
        Ok(0) // TODO fetch from API when payments are enabled in Eigen (PE-305)
    }
}
