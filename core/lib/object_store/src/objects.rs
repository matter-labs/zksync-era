//! Stored objects.

use std::io::{Read, Write};

use anyhow::Context;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use prost::Message;
use zksync_protobuf::{decode, ProtoFmt};
use zksync_types::{
    snapshots::{
        SnapshotFactoryDependencies, SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
    },
    L1BatchNumber,
};

use crate::raw::{BoxedError, Bucket, ObjectStore, ObjectStoreError};

/// Object that can be stored in an [`ObjectStore`].
pub trait StoredObject: Sized {
    /// Bucket in which values are stored.
    const BUCKET: Bucket;
    /// Logical unique key for the object. The lifetime param allows defining keys
    /// that borrow data; see [`CircuitKey`] for an example.
    type Key<'a>: Copy;

    /// Fallback key for the object. If the object is not found, the fallback key is used.
    fn fallback_key(_key: Self::Key<'_>) -> Option<String> {
        None
    }

    /// Encodes the object key to a string.
    fn encode_key(key: Self::Key<'_>) -> String;

    /// Serializes a value to a blob.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails.
    fn serialize(&self) -> Result<Vec<u8>, BoxedError>;

    /// Deserializes a value from the blob.
    ///
    /// # Errors
    ///
    /// Returns an error if deserialization fails.
    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError>;
}

/// Derives [`StoredObject::serialize()`] and [`StoredObject::deserialize()`] using
/// the `bincode` (de)serializer. Should be used in `impl StoredObject` blocks.
#[macro_export]
macro_rules! serialize_using_bincode {
    () => {
        fn serialize(
            &self,
        ) -> std::result::Result<std::vec::Vec<u8>, $crate::_reexports::BoxedError> {
            $crate::bincode::serialize(self).map_err(std::convert::From::from)
        }

        fn deserialize(
            bytes: std::vec::Vec<u8>,
        ) -> std::result::Result<Self, $crate::_reexports::BoxedError> {
            $crate::bincode::deserialize(&bytes).map_err(std::convert::From::from)
        }
    };
}

impl StoredObject for SnapshotFactoryDependencies {
    const BUCKET: Bucket = Bucket::StorageSnapshot;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("snapshot_l1_batch_{key}_factory_deps.proto.gzip")
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        let encoded_bytes = self.build().encode_to_vec();
        encoder.write_all(&encoded_bytes)?;
        encoder.finish().map_err(From::from)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        let mut decoder = GzDecoder::new(&bytes[..]);
        let mut decompressed_bytes = Vec::new();
        decoder
            .read_to_end(&mut decompressed_bytes)
            .map_err(BoxedError::from)?;
        decode(&decompressed_bytes[..])
            .context("deserialization of Message to SnapshotFactoryDependencies")
            .map_err(From::from)
    }
}

impl<K> StoredObject for SnapshotStorageLogsChunk<K>
where
    Self: ProtoFmt,
{
    const BUCKET: Bucket = Bucket::StorageSnapshot;
    type Key<'a> = SnapshotStorageLogsStorageKey;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!(
            "snapshot_l1_batch_{}_storage_logs_part_{:0>4}.proto.gzip",
            key.l1_batch_number, key.chunk_id
        )
    }

    fn serialize(&self) -> Result<Vec<u8>, BoxedError> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        let encoded_bytes = self.build().encode_to_vec();
        encoder.write_all(&encoded_bytes)?;
        encoder.finish().map_err(From::from)
    }

    fn deserialize(bytes: Vec<u8>) -> Result<Self, BoxedError> {
        let mut decoder = GzDecoder::new(&bytes[..]);
        let mut decompressed_bytes = Vec::new();
        decoder
            .read_to_end(&mut decompressed_bytes)
            .map_err(BoxedError::from)?;
        decode(&decompressed_bytes[..])
            .context("deserialization of Message to SnapshotStorageLogsChunk")
            .map_err(From::from)
    }
}

impl dyn ObjectStore + '_ {
    /// Fetches the value for the given key if it exists.
    ///
    /// # Errors
    ///
    /// Returns an error if an object with the `key` does not exist, cannot be accessed,
    /// or cannot be deserialized.
    #[tracing::instrument(
        name = "ObjectStore::get",
        skip_all,
        fields(key) // Will be recorded within the function.
    )]
    pub async fn get<V: StoredObject>(&self, key: V::Key<'_>) -> Result<V, ObjectStoreError> {
        let encoded_key = V::encode_key(key);
        // Record the key for tracing.
        tracing::Span::current().record("key", encoded_key.as_str());
        let bytes = match self.get_raw(V::BUCKET, &encoded_key).await {
            Ok(bytes) => bytes,
            Err(ObjectStoreError::KeyNotFound(e)) => {
                if let Some(fallback_key) = V::fallback_key(key) {
                    self.get_raw(V::BUCKET, &fallback_key).await?
                } else {
                    return Err(ObjectStoreError::KeyNotFound(e));
                }
            }
            Err(e) => return Err(e),
        };
        V::deserialize(bytes).map_err(ObjectStoreError::Serialization)
    }

    /// Fetches the value for the given encoded key if it exists.
    ///
    /// # Errors
    ///
    /// Returns an error if an object with the `encoded_key` does not exist, cannot be accessed,
    /// or cannot be deserialized.
    #[tracing::instrument(
        name = "ObjectStore::get_by_encoded_key",
        skip_all,
        fields(key = %encoded_key)
    )]
    pub async fn get_by_encoded_key<V: StoredObject>(
        &self,
        encoded_key: String,
    ) -> Result<V, ObjectStoreError> {
        let bytes = self.get_raw(V::BUCKET, &encoded_key).await?;
        V::deserialize(bytes).map_err(ObjectStoreError::Serialization)
    }

    /// Stores the value associating it with the key. If the key already exists,
    /// the value is replaced.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or the insertion / replacement operation fails.
    #[tracing::instrument(
        name = "ObjectStore::put",
        skip_all,
        fields(key) // Will be recorded within the function.
    )]
    pub async fn put<V: StoredObject>(
        &self,
        key: V::Key<'_>,
        value: &V,
    ) -> Result<String, ObjectStoreError> {
        let key = V::encode_key(key);
        // Record the key for tracing.
        tracing::Span::current().record("key", key.as_str());
        let bytes = value.serialize().map_err(ObjectStoreError::Serialization)?;
        self.put_raw(V::BUCKET, &key, bytes).await?;
        Ok(key)
    }

    /// Removes a value associated with the key.
    ///
    /// # Errors
    ///
    /// Returns I/O errors specific to the storage.
    #[tracing::instrument(
        name = "ObjectStore::put",
        skip_all,
        fields(key) // Will be recorded within the function.
    )]
    pub async fn remove<V: StoredObject>(&self, key: V::Key<'_>) -> Result<(), ObjectStoreError> {
        let key = V::encode_key(key);
        // Record the key for tracing.
        tracing::Span::current().record("key", key.as_str());
        self.remove_raw(V::BUCKET, &key).await
    }

    pub fn get_storage_prefix<V: StoredObject>(&self) -> String {
        self.storage_prefix_raw(V::BUCKET)
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{
        snapshots::{SnapshotFactoryDependency, SnapshotStorageLog},
        web3::Bytes,
        H256,
    };

    use super::*;
    use crate::MockObjectStore;

    #[test]
    fn test_storage_logs_filesnames_generate_corretly() {
        let filename1 = <SnapshotStorageLogsChunk>::encode_key(SnapshotStorageLogsStorageKey {
            l1_batch_number: L1BatchNumber(42),
            chunk_id: 97,
        });
        let filename2 = <SnapshotStorageLogsChunk>::encode_key(SnapshotStorageLogsStorageKey {
            l1_batch_number: L1BatchNumber(3),
            chunk_id: 531,
        });
        let filename3 = <SnapshotStorageLogsChunk>::encode_key(SnapshotStorageLogsStorageKey {
            l1_batch_number: L1BatchNumber(567),
            chunk_id: 5,
        });
        assert_eq!(
            "snapshot_l1_batch_42_storage_logs_part_0097.proto.gzip",
            filename1
        );
        assert_eq!(
            "snapshot_l1_batch_3_storage_logs_part_0531.proto.gzip",
            filename2
        );
        assert_eq!(
            "snapshot_l1_batch_567_storage_logs_part_0005.proto.gzip",
            filename3
        );
    }

    #[tokio::test]
    async fn test_storage_logs_can_be_serialized_and_deserialized() {
        let store = MockObjectStore::arc();
        let key = SnapshotStorageLogsStorageKey {
            l1_batch_number: L1BatchNumber(567),
            chunk_id: 5,
        };
        let storage_logs = SnapshotStorageLogsChunk {
            storage_logs: vec![
                SnapshotStorageLog {
                    key: H256::random(),
                    value: H256::random(),
                    l1_batch_number_of_initial_write: L1BatchNumber(123),
                    enumeration_index: 234,
                },
                SnapshotStorageLog {
                    key: H256::random(),
                    value: H256::random(),
                    l1_batch_number_of_initial_write: L1BatchNumber(345),
                    enumeration_index: 456,
                },
            ],
        };
        store.put(key, &storage_logs).await.unwrap();
        let reconstructed_storage_logs = store.get(key).await.unwrap();
        assert_eq!(storage_logs, reconstructed_storage_logs);
    }

    #[tokio::test]
    async fn test_factory_deps_can_be_serialized_and_deserialized() {
        let store = MockObjectStore::arc();
        let key = L1BatchNumber(123);
        let factory_deps = SnapshotFactoryDependencies {
            factory_deps: vec![
                SnapshotFactoryDependency {
                    bytecode: Bytes(vec![1, 51, 101, 201, 255]),
                    hash: None,
                },
                SnapshotFactoryDependency {
                    bytecode: Bytes(vec![2, 52, 102, 202, 255]),
                    hash: Some(H256::repeat_byte(1)),
                },
            ],
        };
        store.put(key, &factory_deps).await.unwrap();
        let reconstructed_factory_deps = store.get(key).await.unwrap();
        assert_eq!(factory_deps, reconstructed_factory_deps);
    }
}
