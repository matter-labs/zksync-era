use std::{str::FromStr, sync::Arc};

use rust_eigenda_client::{
    client::BlobProvider,
    config::{PrivateKey, SrsPointsSource},
    EigenClient,
};
use subxt_signer::ExposeSecret;
use url::Url;
use zksync_config::{
    configs::da_client::eigen::{EigenSecrets, PointsSource},
    EigenConfig,
};
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
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
        let eth_rpc_url = match config.eigenda_eth_rpc {
            Some(url) => {
                let url = Url::from_str(url.expose_str())
                    .map_err(|_| anyhow::anyhow!("Invalid eth rpc url"))?;
                Some(rust_eigenda_client::config::SecretUrl::new(url))
            }
            None => None,
        };

        let srs_points_source = match config.points_source {
            PointsSource::Path(path) => SrsPointsSource::Path(path),
            PointsSource::Url(url) => SrsPointsSource::Url(url),
        };

        let eigen_config = rust_eigenda_client::config::EigenConfig {
            disperser_rpc: config.disperser_rpc,
            settlement_layer_confirmation_depth: config.settlement_layer_confirmation_depth,
            eth_rpc_url,
            eigenda_svc_manager_address: config.eigenda_svc_manager_address,
            wait_for_finalization: config.wait_for_finalization,
            authenticated: config.authenticated,
            srs_points_source,
        };
        let private_key = PrivateKey::from_str(secrets.private_key.0.expose_secret())
            .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;
        let eigen_secrets = rust_eigenda_client::config::EigenSecrets { private_key };
        let client = EigenClient::new(eigen_config, eigen_secrets, blob_provider)
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

    async fn balance(&self) -> Result<u64, DAError> {
        Ok(0) // TODO fetch from API when payments are enabled in Eigen (PE-305)
    }
}
