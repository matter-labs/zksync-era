use std::{
    fmt::{Debug, Formatter},
    sync::Arc,
};

use async_trait::async_trait;
use celestia_rpc::{BlobClient, Client};
use celestia_types::{blob::Commitment, nmt::Namespace, Blob, TxConfig};
use serde::{Deserialize, Serialize};
use subxt_signer::ExposeSecret;
use zksync_config::configs::da_client::celestia::{CelestiaConfig, CelestiaSecrets};
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};

use crate::utils::to_non_retriable_da_error;

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Avail network.
#[derive(Clone)]
pub struct CelestiaClient {
    config: CelestiaConfig,
    client: Arc<Client>,
}

impl CelestiaClient {
    pub async fn new(config: CelestiaConfig, secrets: CelestiaSecrets) -> anyhow::Result<Self> {
        let client = Client::new(
            &config.api_node_url,
            Some(secrets.private_key.0.expose_secret()),
        )
        .await
        .expect("could not create Celestia client");

        Ok(Self {
            config: config.clone(),
            client: Arc::new(client),
        })
    }
}
#[derive(Serialize, Deserialize)]
pub struct BlobId {
    pub commitment: Commitment,
    pub height: u64,
}

#[async_trait]
impl DataAvailabilityClient for CelestiaClient {
    async fn dispatch_blob(
        &self,
        _: u32, // batch number
        data: Vec<u8>,
    ) -> Result<DispatchResponse, DAError> {
        let namespace = Namespace::new_v0(self.config.namespace.as_bytes())
            .map_err(to_non_retriable_da_error)?;
        let blob = Blob::new(namespace, data).map_err(to_non_retriable_da_error)?;

        let commitment = blob.commitment;
        let height = self
            .client
            .blob_submit(&[blob], TxConfig::default())
            .await
            .map_err(to_non_retriable_da_error)?;

        let blob_id = BlobId { commitment, height };
        let blob_bytes = bincode::serialize(&blob_id).map_err(to_non_retriable_da_error)?;

        Ok(DispatchResponse {
            blob_id: hex::encode(&blob_bytes),
        })
    }

    async fn get_inclusion_data(&self, _: &str) -> Result<Option<InclusionData>, DAError> {
        Ok(Some(InclusionData { data: vec![] }))
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        Some(1973786) // almost 2MB
    }
}

impl Debug for CelestiaClient {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CelestiaClient")
            .field("config.api_node_url", &self.config.api_node_url)
            .field("config.namespace", &self.config.namespace)
            .finish()
    }
}
