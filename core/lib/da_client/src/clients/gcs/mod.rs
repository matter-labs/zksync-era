use std::{
    fmt,
    fmt::{Debug, Formatter},
    sync::Arc,
};

use zksync_config::ObjectStoreConfig;
use zksync_object_store::{ObjectStore, ObjectStoreError, ObjectStoreFactory};
use zksync_types::{pubdata_da::StorablePubdata, L1BatchNumber};

use crate::{
    types::{DispatchResponse, InclusionData},
    DataAvailabilityInterface,
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

impl DataAvailabilityInterface for GCSDAClient {
    async fn dispatch_blob(
        &self,
        batch_number: L1BatchNumber,
        data: Vec<u8>,
    ) -> Result<DispatchResponse, ObjectStoreError> {
        let key = self
            .object_store
            .put(batch_number, &StorablePubdata { data })
            .await
            .unwrap();

        Ok(DispatchResponse {
            blob_id: key.into_bytes(),
        })
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
