use std::fmt::Debug;

use async_trait::async_trait;
use zksync_config::configs::da_client::eigen_da::EigenDAConfig;
use zksync_da_client::{
    types::{self, DAError, InclusionData},
    DataAvailabilityClient,
};

#[derive(Clone, Debug)]
pub struct EigenDAClient {
    client: reqwest::Client,
    config: EigenDAConfig,
}

impl EigenDAClient {
    pub const BLOB_SIZE_LIMIT_IN_BYTES: usize = 2 * 1024 * 1024; // 2MB

    pub async fn new(config: EigenDAConfig) -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::new(),
            config,
        })
    }
}

#[async_trait]
impl DataAvailabilityClient for EigenDAClient {
    async fn dispatch_blob(
        &self,
        _batch_number: u32,
        blob_data: Vec<u8>,
    ) -> Result<types::DispatchResponse, types::DAError> {
        let api_node_url = match &self.config {
            EigenDAConfig::MemStore(config) => &config.api_node_url,
            EigenDAConfig::Disperser(config) => &config.api_node_url,
        }; // TODO: This should be removed once eigenda proxy is no longer used
        let response = self
            .client
            .post(format!("{}/put/", api_node_url))
            .header(http::header::CONTENT_TYPE, "application/octetstream")
            .body(blob_data)
            .send()
            .await
            .map_err(to_retriable_error)?;

        let request_id = response
            .bytes()
            .await
            .map_err(to_non_retriable_da_error)?
            .to_vec();
        Ok(types::DispatchResponse {
            blob_id: hex::encode(request_id),
        })
    }
    async fn get_inclusion_data(
        &self,
        blob_id: &str,
    ) -> anyhow::Result<Option<types::InclusionData>, types::DAError> {
        let api_node_url = match &self.config {
            EigenDAConfig::MemStore(config) => &config.api_node_url,
            EigenDAConfig::Disperser(config) => &config.api_node_url,
        }; // TODO: This should be removed once eigenda proxy is no longer used
        let response = self
            .client
            .get(format!("{}/get/0x{}", api_node_url, blob_id))
            .send()
            .await
            .map_err(to_retriable_error)?;
        let data = response
            .bytes()
            .await
            .map_err(to_non_retriable_da_error)?
            .to_vec();
        Ok(Some(InclusionData { data }))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(Self::BLOB_SIZE_LIMIT_IN_BYTES)
    }
}

// Note: This methods should be uncommented if the `get_inclusion_data` method
// implementation gets uncommented.
fn to_retriable_error(error: impl Into<anyhow::Error>) -> DAError {
    DAError {
        error: error.into(),
        is_retriable: true,
    }
}

fn to_non_retriable_da_error(error: impl Into<anyhow::Error>) -> DAError {
    DAError {
        error: error.into(),
        is_retriable: false,
    }
}
