use std::{
    fmt,
    fmt::{Debug, Formatter},
    sync::Arc,
};

use async_trait::async_trait;
use zksync_config::ObjectStoreConfig;
use zksync_da_layers::{
    types::{DAError, DispatchResponse, InclusionData},
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
    ) -> Result<DispatchResponse, DAError> {
        if let Err(err) = self
            .object_store
            .put(L1BatchNumber(batch_number), &StorablePubdata { data })
            .await
        {
            return Err(DAError {
                error: anyhow::Error::from(err),
                is_transient: true,
            });
        }

        Ok(DispatchResponse {
            blob_id: batch_number.to_string(),
        })
    }

    async fn get_inclusion_data(&self, key: String) -> Result<Option<InclusionData>, DAError> {
        let key_u32 = key.parse::<u32>().unwrap();
        if let Err(err) = self
            .object_store
            .get::<StorablePubdata>(L1BatchNumber(key_u32))
            .await
        {
            return Err(DAError {
                error: anyhow::Error::from(err),
                is_transient: true,
            });
        }

        // Using default here because we don't get any inclusion data from GCS, thus there's
        // nothing to check on L1.
        return Ok(Some(InclusionData::default()));
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> usize {
        100 * 1024 * 1024 // 100 MB, high enough to not be a problem
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
