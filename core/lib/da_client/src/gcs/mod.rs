use std::{
    fmt,
    fmt::{Debug, Formatter},
    sync::Arc,
};

use async_trait::async_trait;
use zksync_config::ObjectStoreConfig;
use zksync_da_layers::{
    types::{DispatchResponse, InclusionData},
    DataAvailabilityClient,
};
use zksync_object_store::{ObjectStore, ObjectStoreFactory};
use zksync_types::{pubdata_da::StorablePubdata, L1BatchNumber};

/// An implementation of the `DataAvailabilityClient` trait that stores the pubdata in the GCS.
#[derive(Clone)]
pub struct GCSDAClient {
    object_store: Arc<dyn ObjectStore>,
}

impl GCSDAClient {
    pub async fn new(object_store_conf: ObjectStoreConfig) -> Self {
        GCSDAClient {
            object_store: ObjectStoreFactory::create_from_config(&object_store_conf).await,
        }
    }
}

#[async_trait]
impl DataAvailabilityClient for GCSDAClient {
    async fn dispatch_blob(
        &self,
        batch_number: u32,
        data: Vec<u8>,
    ) -> Result<DispatchResponse, anyhow::Error> {
        let key = self
            .object_store
            .put(L1BatchNumber(batch_number), &StorablePubdata { data })
            .await
            .unwrap();

        Ok(DispatchResponse { blob_id: key })
    }

    async fn get_inclusion_data(&self, _: String) -> Result<Option<InclusionData>, anyhow::Error> {
        // Using default here because we don't get any inclusion data from GCS, thus there's
        // nothing to check on L1.
        return Ok(Some(InclusionData::default()));
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }
}

impl Debug for GCSDAClient {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GCSDAClient")
            .field("object_store", &self.object_store)
            .finish()
    }
}
