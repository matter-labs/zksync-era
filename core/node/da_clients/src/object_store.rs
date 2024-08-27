use std::{
    io::{Read, Write},
    sync::Arc,
};

use async_trait::async_trait;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use zksync_config::ObjectStoreConfig;
use zksync_da_client::{
    types::{DAError, DispatchResponse, InclusionData},
    DataAvailabilityClient,
};
use zksync_object_store::{
    Bucket, ObjectStore, ObjectStoreFactory, StoredObject, _reexports::BoxedError,
};
use zksync_types::L1BatchNumber;

/// An implementation of the `DataAvailabilityClient` trait that stores the pubdata in the GCS.
#[derive(Clone, Debug)]
pub struct ObjectStoreDAClient {
    object_store: Arc<dyn ObjectStore>,
}

impl ObjectStoreDAClient {
    pub async fn new(object_store_conf: ObjectStoreConfig) -> anyhow::Result<Self> {
        Ok(ObjectStoreDAClient {
            object_store: ObjectStoreFactory::new(object_store_conf)
                .create_store()
                .await?,
        })
    }
}

#[async_trait]
impl DataAvailabilityClient for ObjectStoreDAClient {
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
                is_retriable: err.is_retriable(),
                error: anyhow::Error::from(err),
            });
        }

        Ok(DispatchResponse {
            blob_id: batch_number.to_string(),
        })
    }

    async fn get_inclusion_data(&self, key: &str) -> Result<Option<InclusionData>, DAError> {
        let key_u32 = key.parse::<u32>().map_err(|err| DAError {
            error: anyhow::Error::from(err).context(format!("Failed to parse blob key: {}", key)),
            is_retriable: false,
        })?;

        if let Err(err) = self
            .object_store
            .get::<StorablePubdata>(L1BatchNumber(key_u32))
            .await
        {
            if let zksync_object_store::ObjectStoreError::KeyNotFound(_) = err {
                return Ok(None);
            }

            return Err(DAError {
                is_retriable: err.is_retriable(),
                error: anyhow::Error::from(err),
            });
        }

        // Using default here because we don't get any inclusion data from object store, thus
        // there's nothing to check on L1.
        return Ok(Some(InclusionData::default()));
    }

    fn clone_boxed(&self) -> Box<dyn DataAvailabilityClient> {
        Box::new(self.clone())
    }

    fn blob_size_limit(&self) -> Option<usize> {
        None
    }
}

/// Used as a wrapper for the pubdata to be stored in the GCS.
#[derive(Debug)]
pub struct StorablePubdata {
    pub data: Vec<u8>,
}

impl StoredObject for StorablePubdata {
    const BUCKET: Bucket = Bucket::DataAvailability;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("l1_batch_{key}_pubdata.gzip")
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&self.data[..])?;
        encoder.finish().map_err(From::from)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        let mut decoder = GzDecoder::new(&bytes[..]);
        let mut decompressed_bytes = Vec::new();
        decoder
            .read_to_end(&mut decompressed_bytes)
            .map_err(BoxedError::from)?;

        Ok(Self {
            data: decompressed_bytes,
        })
    }
}
