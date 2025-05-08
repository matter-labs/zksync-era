use std::{str::FromStr, sync::Arc};

<<<<<<<< HEAD:core/node/da_clients/src/eigenv1m0/client.rs
use rust_eigenda_client::{
    client::BlobProvider,
    config::{PrivateKey, SrsPointsSource},
    EigenClient,
|||||||| 1c8c3a573:core/node/da_clients/src/eigen/client.rs
use rust_eigenda_v2_client::{
    core::{BlobKey, Payload, PayloadForm},
    payload_disperser::{PayloadDisperser, PayloadDisperserConfig},
    utils::{PrivateKey, SecretUrl},
========
use rust_eigenda_v2_client::{
    core::BlobKey,
    payload_disperser::{PayloadDisperser, PayloadDisperserConfig},
    rust_eigenda_signers::signers::private_key::Signer,
    utils::SecretUrl,
>>>>>>>> eigenda-v2-m0:core/node/da_clients/src/eigenv2m0/client.rs
};
use rust_eigenda_v2_common::{Payload, PayloadForm};
use subxt_signer::ExposeSecret;
use url::Url;
use zksync_config::{
<<<<<<<< HEAD:core/node/da_clients/src/eigenv1m0/client.rs
    configs::da_client::eigenv1m0::{EigenSecretsV1M0, PointsSource},
    EigenConfigV1M0,
|||||||| 1c8c3a573:core/node/da_clients/src/eigen/client.rs
    configs::da_client::eigen::{EigenSecrets, PolynomialForm},
    EigenConfig,
========
    configs::da_client::eigenv2m0::{EigenSecretsV2M0, PolynomialForm},
    EigenConfigV2M0,
>>>>>>>> eigenda-v2-m0:core/node/da_clients/src/eigenv2m0/client.rs
};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
    DataAvailabilityClient,
};

use crate::utils::to_retriable_da_error;

// We can't implement DataAvailabilityClient for an outside struct, so it is needed to defined this intermediate struct
#[derive(Debug, Clone)]
<<<<<<<< HEAD:core/node/da_clients/src/eigenv1m0/client.rs
pub struct EigenDAClientV1M0 {
    client: EigenClient,
|||||||| 1c8c3a573:core/node/da_clients/src/eigen/client.rs
pub struct EigenDAClient {
    client: PayloadDisperser,
========
pub struct EigenDAClientV2M0 {
    client: PayloadDisperser,
>>>>>>>> eigenda-v2-m0:core/node/da_clients/src/eigenv2m0/client.rs
}

<<<<<<<< HEAD:core/node/da_clients/src/eigenv1m0/client.rs
impl EigenDAClientV1M0 {
    pub async fn new(
        config: EigenConfigV1M0,
        secrets: EigenSecretsV1M0,
        blob_provider: Arc<dyn BlobProvider>,
    ) -> anyhow::Result<Self> {
|||||||| 1c8c3a573:core/node/da_clients/src/eigen/client.rs
impl EigenDAClient {
    pub async fn new(config: EigenConfig, secrets: EigenSecrets) -> anyhow::Result<Self> {
========
impl EigenDAClientV2M0 {
    pub async fn new(config: EigenConfigV2M0, secrets: EigenSecretsV2M0) -> anyhow::Result<Self> {
>>>>>>>> eigenda-v2-m0:core/node/da_clients/src/eigenv2m0/client.rs
        let url = Url::from_str(
            config
                .eigenda_eth_rpc
                .ok_or(anyhow::anyhow!("Eigenda eth rpc url is not set"))?
                .expose_str(),
        )
        .map_err(|_| anyhow::anyhow!("Invalid eth rpc url"))?;
        let eth_rpc_url = rust_eigenda_client::config::SecretUrl::new(url);

        let srs_points_source = match config.points_source {
            PointsSource::Path(path) => SrsPointsSource::Path(path),
            PointsSource::Url(url) => SrsPointsSource::Url(url),
        };

<<<<<<<< HEAD:core/node/da_clients/src/eigenv1m0/client.rs
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
        let private_key = PrivateKey::from_str(secrets.private_key.0.expose_secret())
|||||||| 1c8c3a573:core/node/da_clients/src/eigen/client.rs
        let payload_disperser_config = PayloadDisperserConfig {
            polynomial_form: payload_form,
            blob_version: config.blob_version,
            cert_verifier_address: config.cert_verifier_addr,
            eth_rpc_url: SecretUrl::new(url),
            disperser_rpc: config.disperser_rpc,
            use_secure_grpc_flag: config.authenticated,
        };
        let private_key = PrivateKey::from_str(secrets.private_key.0.expose_secret())
========
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
>>>>>>>> eigenda-v2-m0:core/node/da_clients/src/eigenv2m0/client.rs
            .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;
<<<<<<<< HEAD:core/node/da_clients/src/eigenv1m0/client.rs
        let eigen_secrets = rust_eigenda_client::config::EigenSecrets { private_key };
        let client = EigenClient::new(eigen_config, eigen_secrets, blob_provider)
|||||||| 1c8c3a573:core/node/da_clients/src/eigen/client.rs
        let client = PayloadDisperser::new(payload_disperser_config, private_key)
========
        let signer = Signer::new(private_key);
        let client = PayloadDisperser::new(payload_disperser_config, signer)
>>>>>>>> eigenda-v2-m0:core/node/da_clients/src/eigenv2m0/client.rs
            .await
            .map_err(|e| anyhow::anyhow!("Eigen client Error: {:?}", e))?;
        Ok(Self { client })
    }
}

#[async_trait::async_trait]
<<<<<<<< HEAD:core/node/da_clients/src/eigenv1m0/client.rs
impl DataAvailabilityClient for EigenDAClientV1M0 {
|||||||| 1c8c3a573:core/node/da_clients/src/eigen/client.rs
impl DataAvailabilityClient for EigenDAClient {
========
impl DataAvailabilityClient for EigenDAClientV2M0 {
>>>>>>>> eigenda-v2-m0:core/node/da_clients/src/eigenv2m0/client.rs
    async fn dispatch_blob(
        &self,
        _: u32, // batch number
        data: Vec<u8>,
    ) -> Result<DispatchResponse, DAError> {
        let blob_id = self
            .client
            .dispatch_blob(data)
            .await
            .map_err(to_retriable_da_error)?;

        Ok(DispatchResponse::from(blob_id))
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
        let inclusion_data = self
            .client
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

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
<<<<<<<< HEAD:core/node/da_clients/src/eigenv1m0/client.rs
        self.client.blob_size_limit()
|||||||| 1c8c3a573:core/node/da_clients/src/eigen/client.rs
        PayloadDisperser::blob_size_limit()
========
        PayloadDisperser::<Signer>::blob_size_limit()
>>>>>>>> eigenda-v2-m0:core/node/da_clients/src/eigenv2m0/client.rs
    }

    fn client_type(&self) -> ClientType {
<<<<<<<< HEAD:core/node/da_clients/src/eigenv1m0/client.rs
        ClientType::EigenV1M0
|||||||| 1c8c3a573:core/node/da_clients/src/eigen/client.rs
        ClientType::Eigen
========
        ClientType::EigenV2M0
>>>>>>>> eigenda-v2-m0:core/node/da_clients/src/eigenv2m0/client.rs
    }

    async fn balance(&self) -> Result<u64, DAError> {
        Ok(0) // TODO fetch from API when payments are enabled in Eigen (PE-305)
    }
}
