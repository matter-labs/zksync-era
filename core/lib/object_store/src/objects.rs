//! Stored objects.

use std::io::{Read, Write};

use anyhow::Context;
use flate2::{read::GzDecoder, write::GzEncoder, Compression};
use prost::Message;
use zksync_protobuf::{decode, ProtoFmt};
use zksync_types::{
    aggregated_operations::L1BatchProofForL1,
    proofs::{AggregationRound, PrepareBasicCircuitsJob},
    snapshots::{
        SnapshotFactoryDependencies, SnapshotStorageLogsChunk, SnapshotStorageLogsStorageKey,
    },
    storage::witness_block_state::WitnessBlockState,
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

impl StoredObject for SnapshotStorageLogsChunk {
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

impl StoredObject for WitnessBlockState {
    const BUCKET: Bucket = Bucket::WitnessInput;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("witness_block_state_for_l1_batch_{key}.bin")
    }

    serialize_using_bincode!();
}

impl StoredObject for PrepareBasicCircuitsJob {
    const BUCKET: Bucket = Bucket::WitnessInput;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("merkel_tree_paths_{key}.bin")
    }

    serialize_using_bincode!();
}

/// Storage key for a [AggregationWrapper`].
#[derive(Debug, Clone, Copy)]
pub struct AggregationsKey {
    pub block_number: L1BatchNumber,
    pub circuit_id: u8,
    pub depth: u16,
}

/// Storage key for a [ClosedFormInputWrapper`].
#[derive(Debug, Clone, Copy)]
pub struct ClosedFormInputKey {
    pub block_number: L1BatchNumber,
    pub circuit_id: u8,
}

/// Storage key for a [`CircuitWrapper`].
#[derive(Debug, Clone, Copy)]
pub struct FriCircuitKey {
    pub block_number: L1BatchNumber,
    pub sequence_number: usize,
    pub circuit_id: u8,
    pub aggregation_round: AggregationRound,
    pub depth: u16,
}

/// Storage key for a [`ZkSyncCircuit`].
#[derive(Debug, Clone, Copy)]
pub struct CircuitKey<'a> {
    pub block_number: L1BatchNumber,
    pub sequence_number: usize,
    pub circuit_type: &'a str,
    pub aggregation_round: AggregationRound,
}

impl StoredObject for L1BatchProofForL1 {
    const BUCKET: Bucket = Bucket::ProofsFri;
    type Key<'a> = L1BatchNumber;

    fn encode_key(key: Self::Key<'_>) -> String {
        format!("l1_batch_proof_{key}.bin")
    }

    serialize_using_bincode!();
}

impl dyn ObjectStore + '_ {
    /// Fetches the value for the given key if it exists.
    ///
    /// # Errors
    ///
    /// Returns an error if an object with the `key` does not exist, cannot be accessed,
    /// or cannot be deserialized.
    pub async fn get<V: StoredObject>(&self, key: V::Key<'_>) -> Result<V, ObjectStoreError> {
        let key = V::encode_key(key);
        let bytes = self.get_raw(V::BUCKET, &key).await?;
        V::deserialize(bytes).map_err(ObjectStoreError::Serialization)
    }

    /// Stores the value associating it with the key. If the key already exists,
    /// the value is replaced.
    ///
    /// # Errors
    ///
    /// Returns an error if serialization or the insertion / replacement operation fails.
    pub async fn put<V: StoredObject>(
        &self,
        key: V::Key<'_>,
        value: &V,
    ) -> Result<String, ObjectStoreError> {
        let key = V::encode_key(key);
        let bytes = value.serialize().map_err(ObjectStoreError::Serialization)?;
        self.put_raw(V::BUCKET, &key, bytes).await?;
        Ok(key)
    }

    pub fn get_storage_prefix<V: StoredObject>(&self) -> String {
        self.storage_prefix_raw(V::BUCKET)
    }
}

#[cfg(test)]
mod tests {
    use zksync_types::{
        snapshots::{SnapshotFactoryDependency, SnapshotStorageLog},
        AccountTreeId, Bytes, StorageKey, H160, H256,
    };

    use super::*;
    use crate::ObjectStoreFactory;

    #[test]
    fn test_storage_logs_filesnames_generate_corretly() {
        let filename1 = SnapshotStorageLogsChunk::encode_key(SnapshotStorageLogsStorageKey {
            l1_batch_number: L1BatchNumber(42),
            chunk_id: 97,
        });
        let filename2 = SnapshotStorageLogsChunk::encode_key(SnapshotStorageLogsStorageKey {
            l1_batch_number: L1BatchNumber(3),
            chunk_id: 531,
        });
        let filename3 = SnapshotStorageLogsChunk::encode_key(SnapshotStorageLogsStorageKey {
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
        let store = ObjectStoreFactory::mock().create_store().await;
        let key = SnapshotStorageLogsStorageKey {
            l1_batch_number: L1BatchNumber(567),
            chunk_id: 5,
        };
        let storage_logs = SnapshotStorageLogsChunk {
            storage_logs: vec![
                SnapshotStorageLog {
                    key: StorageKey::new(AccountTreeId::new(H160::random()), H256::random()),
                    value: H256::random(),
                    l1_batch_number_of_initial_write: L1BatchNumber(123),
                    enumeration_index: 234,
                },
                SnapshotStorageLog {
                    key: StorageKey::new(AccountTreeId::new(H160::random()), H256::random()),
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
        let store = ObjectStoreFactory::mock().create_store().await;
        let key = L1BatchNumber(123);
        let factory_deps = SnapshotFactoryDependencies {
            factory_deps: vec![
                SnapshotFactoryDependency {
                    bytecode: Bytes(vec![1, 51, 101, 201, 255]),
                },
                SnapshotFactoryDependency {
                    bytecode: Bytes(vec![2, 52, 102, 202, 255]),
                },
            ],
        };
        store.put(key, &factory_deps).await.unwrap();
        let reconstructed_factory_deps = store.get(key).await.unwrap();
        assert_eq!(factory_deps, reconstructed_factory_deps);
    }
}
