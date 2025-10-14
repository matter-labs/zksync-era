use std::sync::Arc;

use zksync_node_framework::{FromContext, Resource, WiringError, WiringLayer};
use zksync_object_store::ObjectStore;

use crate::{BlobClient, BlobHttpClient, LocalStorageBlobSource};

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum BlobClientMode {
    Local,
    Blobscan,
}

/// Wiring layer for blob source.
#[derive(Debug, Clone)]
pub struct BlobClientLayer {
    pub mode: BlobClientMode,
    pub blobscan_url: Option<String>,
}

#[derive(Debug, FromContext)]
pub struct Input {
    pub object_store: Option<Arc<dyn ObjectStore>>,
}

#[async_trait::async_trait]
impl WiringLayer for BlobClientLayer {
    type Input = Input;
    type Output = BlobClientResource;

    fn layer_name(&self) -> &'static str {
        "blob_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let blob_client: Arc<dyn BlobClient> = match self.mode {
            BlobClientMode::Local => {
                Arc::new(LocalStorageBlobSource::new(input.object_store.unwrap()))
            }
            BlobClientMode::Blobscan => {
                Arc::new(BlobHttpClient::new(&self.blobscan_url.unwrap()).unwrap())
            }
        };
        let resource = BlobClientResource(blob_client);
        Ok(resource)
    }
}

/// A resource that provides [`BlobClient`] implementation to the service.
#[derive(Debug, Clone)]
pub struct BlobClientResource(pub Arc<dyn BlobClient>);

impl Resource for BlobClientResource {
    fn name() -> String {
        "common/blob_client".into()
    }
}

impl<T: BlobClient> From<Arc<T>> for BlobClientResource {
    fn from(provider: Arc<T>) -> Self {
        Self(provider)
    }
}
