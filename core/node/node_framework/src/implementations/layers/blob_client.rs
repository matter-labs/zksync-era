use std::sync::Arc;

use zksync_l1_recovery::LocalStorageBlobSource;

use crate::{
    implementations::resources::{
        blob_client::BlobClientResource, object_store::ObjectStoreResource,
    },
    wiring_layer::{WiringError, WiringLayer},
};

/// Wiring layer for object store.
#[derive(Debug)]
pub struct BlobClientLayer {}

impl BlobClientLayer {
    pub fn new() -> Self {
        Self {}
    }
}

#[async_trait::async_trait]
impl WiringLayer for BlobClientLayer {
    type Input = ObjectStoreResource;
    type Output = BlobClientResource;

    fn layer_name(&self) -> &'static str {
        "blob_client_layer"
    }

    async fn wire(self, input: Self::Input) -> Result<Self::Output, WiringError> {
        let blob_client = Arc::new(LocalStorageBlobSource::new(input.0));
        let resource = BlobClientResource(blob_client);
        Ok(resource)
    }
}
