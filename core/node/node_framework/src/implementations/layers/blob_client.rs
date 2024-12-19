use std::sync::Arc;

use zksync_l1_recovery::{BlobClient, BlobHttpClient, LocalStorageBlobSource};

use crate::{
    implementations::resources::{
        blob_client::BlobClientResource, object_store::ObjectStoreResource,
    },
    wiring_layer::{WiringError, WiringLayer},
    FromContext,
};

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

#[derive(Debug, Clone, FromContext)]
#[context(crate = crate)]
pub struct Input {
    pub object_store: Option<ObjectStoreResource>,
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
                Arc::new(LocalStorageBlobSource::new(input.object_store.unwrap().0))
            }
            BlobClientMode::Blobscan => {
                Arc::new(BlobHttpClient::new(&self.blobscan_url.unwrap()).unwrap())
            }
        };
        let resource = BlobClientResource(blob_client);
        Ok(resource)
    }
}
