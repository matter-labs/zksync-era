use std::{
    fmt::{Debug, Formatter},
    str::FromStr,
    sync::Arc,
};

use async_trait::async_trait;
use celestia_types::{blob::Commitment, nmt::Namespace, Blob};
use chrono::{DateTime, Utc};
use secrecy::ExposeSecret;
use serde::{Deserialize, Serialize};
use tonic::transport::Endpoint;
use zksync_config::configs::da_client::celestia::CelestiaConfig;
use zksync_da_client::{
    types::{ClientType, DAError, DispatchResponse, FinalityResponse, InclusionData},
    DataAvailabilityClient,
};

use crate::{
    celestia::sdk::{BlobTxHash, RawCelestiaClient},
    utils::to_non_retriable_da_error,
};

/// An implementation of the `DataAvailabilityClient` trait that interacts with the Avail network.
#[derive(Clone)]
pub struct CelestiaClient {
    config: CelestiaConfig,
    client: Arc<RawCelestiaClient>,
}

impl CelestiaClient {
    pub async fn new(config: CelestiaConfig) -> anyhow::Result<Self> {
        let grpc_channel = Endpoint::from_str(config.api_node_url.clone().as_str())?
            .timeout(config.timeout)
            .connect()
            .await?;

        let private_key = config.private_key.0.expose_secret().to_owned();
        let client = RawCelestiaClient::new(grpc_channel, private_key, config.chain_id.clone())
            .expect("could not create Celestia client");

        Ok(Self {
            config,
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
        let namespace_bytes =
            hex::decode(&self.config.namespace).map_err(to_non_retriable_da_error)?;
        let namespace =
            Namespace::new_v0(namespace_bytes.as_slice()).map_err(to_non_retriable_da_error)?;
        let blob = Blob::new(namespace, data).map_err(to_non_retriable_da_error)?;

        let commitment = blob.commitment;
        let blob_tx = self
            .client
            .prepare(vec![blob])
            .await
            .map_err(to_non_retriable_da_error)?;

        let blob_tx_hash = BlobTxHash::compute(&blob_tx);
        let height = self
            .client
            .submit(blob_tx_hash, blob_tx)
            .await
            .map_err(to_non_retriable_da_error)?;

        let blob_id = BlobId { commitment, height };
        let blob_bytes = bincode::serialize(&blob_id).map_err(to_non_retriable_da_error)?;

        Ok(DispatchResponse {
            request_id: hex::encode(&blob_bytes),
        })
    }

    async fn ensure_finality(
        &self,
        dispatch_request_id: String,
        _: DateTime<Utc>,
    ) -> Result<Option<FinalityResponse>, DAError> {
        // TODO: return a quick confirmation in `dispatch_blob` and await here
        Ok(Some(FinalityResponse {
            blob_id: dispatch_request_id,
        }))
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

    fn client_type(&self) -> ClientType {
        ClientType::Celestia
    }

    async fn balance(&self) -> Result<u64, DAError> {
        self.client
            .balance()
            .await
            .map_err(to_non_retriable_da_error)
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
