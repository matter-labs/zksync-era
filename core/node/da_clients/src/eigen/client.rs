use std::{str::FromStr, sync::Arc};

use async_trait::async_trait;
use secp256k1::SecretKey;
use subxt_signer::ExposeSecret;
use zksync_config::{configs::da_client::eigen::EigenSecrets, EigenConfig};
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};

use super::sdk::RawEigenClient;
use crate::utils::to_retriable_da_error;

#[async_trait]
pub trait GetBlobData: Clone + std::fmt::Debug + Send + Sync {
    async fn call(&self, input: &str) -> anyhow::Result<Option<Vec<u8>>>;
}

/// EigenClient is a client for the Eigen DA service.
#[derive(Debug, Clone)]
pub struct EigenClient<T: GetBlobData> {
    pub(crate) client: Arc<RawEigenClient<T>>,
}

impl<T: GetBlobData> EigenClient<T> {
    pub async fn new(
        config: EigenConfig,
        secrets: EigenSecrets,
        get_blob_data: Box<T>,
    ) -> anyhow::Result<Self> {
        let private_key = SecretKey::from_str(secrets.private_key.0.expose_secret().as_str())
            .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?;

        let client = RawEigenClient::new(private_key, config, get_blob_data).await?;
        Ok(Self {
            client: Arc::new(client),
        })
    }
}

#[async_trait]
impl<T: GetBlobData + 'static> DataAvailabilityClient for EigenClient<T> {
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
        Some(RawEigenClient::<T>::blob_size_limit())
    }
}
