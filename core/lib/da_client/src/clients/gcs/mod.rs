use std::{
    fmt,
    fmt::{Debug, Formatter},
    sync::Arc,
};

use zksync_config::ObjectStoreConfig;
use zksync_object_store::{ObjectStore, ObjectStoreError, ObjectStoreFactory};

use crate::{
    types::{DispatchResponse, InclusionData},
    DataAvailabilityClient,
};

struct GCSDAClient {
    object_store: Arc<dyn ObjectStore>,
}

impl GCSDAClient {
    pub fn new(object_store_conf: ObjectStoreConfig) -> Self {
        GCSDAClient {
            object_store: ObjectStoreFactory::create_from_config(&object_store_conf),
        }
    }
}

impl DataAvailabilityClient<ObjectStoreError> for GCSDAClient {
    fn dispatch_blob(&self, data: Vec<u8>) -> Result<DispatchResponse, ObjectStoreError> {
        Ok(DispatchResponse::default())
    }

    fn get_inclusion_data(&self, _: Vec<u8>) -> Result<InclusionData, ObjectStoreError> {
        return Ok(InclusionData::default());
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
