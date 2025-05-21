use std::{str::FromStr, sync::Arc};

use rust_eigenda_client::{
    client::BlobProvider, config::SrsPointsSource, rust_eigenda_signers::{signers::private_key::Signer, SecretKey}, EigenClient
};
use subxt_signer::ExposeSecret;
use url::Url;
use zksync_config::{
    configs::da_client::eigen::{EigenSecrets, PointsSource},
    EigenConfig,
};
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
    DataAvailabilityClient,
};

use crate::utils::to_retriable_da_error;

// We can't implement DataAvailabilityClient for an outside struct, so it is needed to defined this intermediate struct
#[derive(Debug, Clone)]
pub struct EigenDAClient {
    client: EigenClient,
}

impl EigenDAClient {
    pub async fn new(
        config: EigenConfig,
        secrets: EigenSecrets,
        blob_provider: Arc<dyn BlobProvider>,
    ) -> anyhow::Result<Self> {
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
        let signer = Signer::new(SecretKey::from_str(secrets.private_key.0.expose_secret()).map_err(|_| {
            anyhow::anyhow!("Invalid private key")
        })?);

        let client = EigenClient::new(eigen_config, signer, blob_provider)
            .await
            .map_err(|e| anyhow::anyhow!("Eigen client Error: {:?}", e))?;
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
        let finalized = self
            .client
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
        self.client.blob_size_limit()
    }

    fn client_type(&self) -> ClientType {
        ClientType::Eigen
    }

    async fn balance(&self) -> Result<u64, DAError> {
        Ok(0) // TODO fetch from API when payments are enabled in Eigen (PE-305)
    }
}
