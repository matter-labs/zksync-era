use std::{fmt::Debug, sync::Arc};

use async_trait::async_trait;
use jsonrpsee::ws_client::WsClientBuilder;
use subxt_signer::ExposeSecret;
use zksync_config::configs::da_client::avail::{AvailConfig, AvailSecrets};
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};

use crate::avail::sdk::RawAvailClient;

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Avail network.
#[derive(Debug, Clone)]
pub struct AvailClient {
    config: AvailConfig,
    sdk_client: Arc<RawAvailClient>,
}

impl AvailClient {
    pub async fn new(config: AvailConfig, secrets: AvailSecrets) -> anyhow::Result<Self> {
        let seed_phrase = secrets
            .seed_phrase
            .ok_or_else(|| anyhow::anyhow!("seed phrase"))?;
        let sdk_client = RawAvailClient::new(config.app_id, seed_phrase.0.expose_secret()).await?;

        Ok(Self {
            config,
            sdk_client: Arc::new(sdk_client),
        })
    }
}

#[async_trait]
impl DataAvailabilityClient for AvailClient {
    async fn dispatch_blob(
        &self,
        _: u32, // batch_number
        data: Vec<u8>,
    ) -> anyhow::Result<DispatchResponse, DAError> {
        let client = WsClientBuilder::default()
            .build(self.config.api_node_url.as_str())
            .await
            .map_err(to_non_retriable_da_error)?;

        let extrinsic = self
            .sdk_client
            .build_extrinsic(&client, data)
            .await
            .map_err(to_non_retriable_da_error)?;

        let block_hash = self
            .sdk_client
            .submit_extrinsic(&client, extrinsic.as_str())
            .await
            .map_err(to_non_retriable_da_error)?;
        let tx_id = self
            .sdk_client
            .get_tx_id(&client, block_hash.as_str(), extrinsic.as_str())
            .await
            .map_err(to_non_retriable_da_error)?;

        Ok(DispatchResponse::from(format!("{}:{}", block_hash, tx_id)))
    }

    async fn get_inclusion_data(
        &self,
        _blob_id: &str,
    ) -> anyhow::Result<Option<InclusionData>, DAError> {
        // TODO: implement inclusion data retrieval
        Ok(Some(InclusionData { data: vec![] }))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(RawAvailClient::MAX_BLOB_SIZE)
    }
}

pub fn to_non_retriable_da_error(error: impl Into<anyhow::Error>) -> DAError {
    DAError {
        error: error.into(),
        is_retriable: false,
    }
}
