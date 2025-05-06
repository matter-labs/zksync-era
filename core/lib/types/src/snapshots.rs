use std::ops;

use num_enum::{IntoPrimitive, TryFromPrimitive};
use serde::{Deserialize, Serialize};
use zksync_basic_types::{L1BatchNumber, L2BlockNumber, H256};

use crate::{u256_to_h256, utils, web3::Bytes, ProtocolVersionId, StorageKey, StorageValue, U256};

/// Information about all snapshots persisted by the node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllSnapshots {
    /// L1 batch numbers for complete snapshots. Ordered by descending number (i.e., 0th element
    /// corresponds to the newest snapshot).
    pub snapshots_l1_batch_numbers: Vec<L1BatchNumber>,
}

/// Version of snapshot influencing the format of data stored in GCS.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[derive(TryFromPrimitive, IntoPrimitive)]
#[repr(u16)]
pub enum SnapshotVersion {
    /// Initial snapshot version. Keys in storage logs are stored as `(address, key)` pairs.
    Version0 = 0,
    /// Snapshot version made compatible with L1 recovery. Differs from `Version0` by including
    /// hashed keys in storage logs instead of `(address, key)` pairs.
    Version1 = 1,
}

/// Storage snapshot metadata. Used in DAL to fetch certain snapshot data.
#[derive(Debug, Clone)]
pub struct SnapshotMetadata {
    pub version: SnapshotVersion,
    /// L1 batch for the snapshot. The data in the snapshot captures node storage at the end of this batch.
    pub l1_batch_number: L1BatchNumber,
    /// Path to the factory dependencies blob.
    pub factory_deps_filepath: String,
    /// Paths to the storage log blobs. Ordered by the chunk ID. If a certain chunk is not produced yet,
    /// the corresponding path is `None`.
    pub storage_logs_filepaths: Vec<Option<String>>,
}

impl SnapshotMetadata {
    /// Checks whether a snapshot is complete (contains all information to restore from).
    pub fn is_complete(&self) -> bool {
        self.storage_logs_filepaths.iter().all(Option::is_some)
    }
}

/// Snapshot data returned by using JSON-RPC API.
/// Contains all data not contained in `factory_deps` / `storage_logs` files to perform restore process.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHeader {
    // Not a `SnapshotVersion` to have controllable error handling in case of deserializing a header on an outdated node.
    #[serde(default)]
    pub version: u16,
    pub l1_batch_number: L1BatchNumber,
    #[serde(rename = "miniblockNumber")] // legacy naming
    pub l2_block_number: L2BlockNumber,
    /// Ordered by chunk IDs.
    pub storage_logs_chunks: Vec<SnapshotStorageLogsChunkMetadata>,
    pub factory_deps_filepath: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStorageLogsChunkMetadata {
    pub chunk_id: u64,
    // can be either be a file available under HTTP(s) or local filesystem path
    pub filepath: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStorageLogsStorageKey {
    pub l1_batch_number: L1BatchNumber,
    pub chunk_id: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SnapshotStorageLogsChunk<K = H256> {
    pub storage_logs: Vec<SnapshotStorageLog<K>>,
}

/// Storage log record in a storage snapshot.
///
/// Version 0 and version 1 snapshots differ in the key type; version 0 uses full [`StorageKey`]s (i.e., storage key preimages),
/// and version 1 uses [`H256`] hashed keys. See [`SnapshotVersion`] for details.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotStorageLog<K = H256> {
    pub key: K,
    pub value: StorageValue,
    pub l1_batch_number_of_initial_write: L1BatchNumber,
    pub enumeration_index: u64,
}

impl SnapshotStorageLog<StorageKey> {
    pub fn drop_key_preimage(self) -> SnapshotStorageLog {
        SnapshotStorageLog {
            key: self.key.hashed_key(),
            value: self.value,
            l1_batch_number_of_initial_write: self.l1_batch_number_of_initial_write,
            enumeration_index: self.enumeration_index,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct SnapshotFactoryDependencies {
    pub factory_deps: Vec<SnapshotFactoryDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotFactoryDependency {
    pub bytecode: Bytes,
    pub hash: Option<H256>,
}

#[cfg(feature = "protobuf")]
mod proto_impl {
    use anyhow::Context;
    use zksync_basic_types::AccountTreeId;
    use zksync_protobuf::{required, ProtoFmt};

    use super::*;

    impl ProtoFmt for SnapshotFactoryDependency {
        type Proto = crate::proto::SnapshotFactoryDependency;

        fn read(r: &Self::Proto) -> anyhow::Result<Self> {
            Ok(Self {
                bytecode: Bytes(required(&r.bytecode).context("bytecode")?.clone()),
                hash: if let Some(raw) = &r.hash {
                    anyhow::ensure!(raw.len() == 32, "hash.len");
                    Some(H256::from_slice(raw))
                } else {
                    None
                },
            })
        }

        fn build(&self) -> Self::Proto {
            Self::Proto {
                bytecode: Some(self.bytecode.0.as_slice().into()),
                hash: self.hash.map(|hash| hash.0.to_vec()),
            }
        }
    }

    impl ProtoFmt for SnapshotFactoryDependencies {
        type Proto = crate::proto::SnapshotFactoryDependencies;

        fn read(r: &Self::Proto) -> anyhow::Result<Self> {
            let mut factory_deps = Vec::with_capacity(r.factory_deps.len());
            for (i, factory_dep) in r.factory_deps.iter().enumerate() {
                factory_deps.push(
                    SnapshotFactoryDependency::read(factory_dep)
                        .with_context(|| format!("factory_deps[{i}]"))?,
                )
            }
            Ok(Self { factory_deps })
        }
        fn build(&self) -> Self::Proto {
            Self::Proto {
                factory_deps: self
                    .factory_deps
                    .iter()
                    .map(SnapshotFactoryDependency::build)
                    .collect(),
            }
        }
    }

    impl ProtoFmt for SnapshotStorageLog {
        type Proto = crate::proto::SnapshotStorageLog;

        fn read(r: &Self::Proto) -> anyhow::Result<Self> {
            let hashed_key = if let Some(hashed_key) = &r.hashed_key {
                <[u8; 32]>::try_from(hashed_key.as_slice())
                    .context("hashed_key")?
                    .into()
            } else {
                let address = required(&r.account_address)
                    .and_then(|bytes| Ok(<[u8; 20]>::try_from(bytes.as_slice())?.into()))
                    .context("account_address")?;
                let key = required(&r.storage_key)
                    .and_then(|bytes| Ok(<[u8; 32]>::try_from(bytes.as_slice())?.into()))
                    .context("storage_key")?;
                StorageKey::new(AccountTreeId::new(address), key).hashed_key()
            };

            Ok(Self {
                key: hashed_key,
                value: required(&r.storage_value)
                    .and_then(|bytes| Ok(<[u8; 32]>::try_from(bytes.as_slice())?.into()))
                    .context("storage_value")?,
                l1_batch_number_of_initial_write: L1BatchNumber(
                    *required(&r.l1_batch_number_of_initial_write)
                        .context("l1_batch_number_of_initial_write")?,
                ),
                enumeration_index: *required(&r.enumeration_index).context("enumeration_index")?,
            })
        }

        fn build(&self) -> Self::Proto {
            Self::Proto {
                account_address: None,
                storage_key: None,
                hashed_key: Some(self.key.as_bytes().to_vec()),
                storage_value: Some(self.value.as_bytes().to_vec()),
                l1_batch_number_of_initial_write: Some(self.l1_batch_number_of_initial_write.0),
                enumeration_index: Some(self.enumeration_index),
            }
        }
    }

    impl ProtoFmt for SnapshotStorageLog<StorageKey> {
        type Proto = crate::proto::SnapshotStorageLog;

        fn read(r: &Self::Proto) -> anyhow::Result<Self> {
            let address = required(&r.account_address)
                .and_then(|bytes| Ok(<[u8; 20]>::try_from(bytes.as_slice())?.into()))
                .context("account_address")?;
            let key = required(&r.storage_key)
                .and_then(|bytes| Ok(<[u8; 32]>::try_from(bytes.as_slice())?.into()))
                .context("storage_key")?;

            Ok(Self {
                key: StorageKey::new(AccountTreeId::new(address), key),
                value: required(&r.storage_value)
                    .and_then(|bytes| Ok(<[u8; 32]>::try_from(bytes.as_slice())?.into()))
                    .context("storage_value")?,
                l1_batch_number_of_initial_write: L1BatchNumber(
                    *required(&r.l1_batch_number_of_initial_write)
                        .context("l1_batch_number_of_initial_write")?,
                ),
                enumeration_index: *required(&r.enumeration_index).context("enumeration_index")?,
            })
        }

        fn build(&self) -> Self::Proto {
            Self::Proto {
                account_address: Some(self.key.address().as_bytes().to_vec()),
                storage_key: Some(self.key.key().as_bytes().to_vec()),
                hashed_key: None,
                storage_value: Some(self.value.as_bytes().to_vec()),
                l1_batch_number_of_initial_write: Some(self.l1_batch_number_of_initial_write.0),
                enumeration_index: Some(self.enumeration_index),
            }
        }
    }

    impl<K> ProtoFmt for SnapshotStorageLogsChunk<K>
    where
        SnapshotStorageLog<K>: ProtoFmt<Proto = crate::proto::SnapshotStorageLog>,
    {
        type Proto = crate::proto::SnapshotStorageLogsChunk;

        fn read(r: &Self::Proto) -> anyhow::Result<Self> {
            let mut storage_logs = Vec::with_capacity(r.storage_logs.len());
            for (i, storage_log) in r.storage_logs.iter().enumerate() {
                storage_logs.push(
                    SnapshotStorageLog::<K>::read(storage_log)
                        .with_context(|| format!("storage_log[{i}]"))?,
                )
            }
            Ok(Self { storage_logs })
        }

        fn build(&self) -> Self::Proto {
            Self::Proto {
                storage_logs: self
                    .storage_logs
                    .iter()
                    .map(SnapshotStorageLog::<K>::build)
                    .collect(),
            }
        }
    }
}

/// Status of snapshot recovery process stored in Postgres.
#[derive(derive_more::Debug, PartialEq)]
pub struct SnapshotRecoveryStatus {
    pub l1_batch_number: L1BatchNumber,
    pub l1_batch_root_hash: H256,
    #[debug("{} (raw: {})", utils::display_timestamp(*l1_batch_timestamp), l1_batch_timestamp)]
    pub l1_batch_timestamp: u64,
    pub l2_block_number: L2BlockNumber,
    pub l2_block_hash: H256,
    #[debug("{} (raw: {})", utils::display_timestamp(*l2_block_timestamp), l2_block_timestamp)]
    pub l2_block_timestamp: u64,
    pub protocol_version: ProtocolVersionId,
    #[debug(
        "{{ len: {}, left: {} }}",
        storage_logs_chunks_processed.len(),
        self.storage_logs_chunks_left_to_process()
    )]
    pub storage_logs_chunks_processed: Vec<bool>,
}

impl SnapshotRecoveryStatus {
    /// Returns the number of storage log chunks left to process.
    pub fn storage_logs_chunks_left_to_process(&self) -> usize {
        self.storage_logs_chunks_processed
            .iter()
            .filter(|&&is_processed| !is_processed)
            .count()
    }
}

/// Returns a chunk of `hashed_keys` with 0-based index `chunk_id` among `count`.
///
/// Chunks do not intersect and jointly cover the entire `hashed_key` space. If `hashed_key`s
/// are uniformly distributed (which is the case), the returned ranges are expected to contain
/// the same number of entries.
///
/// Used by multiple components during snapshot creation and recovery.
///
/// # Panics
///
/// Panics if `chunk_count == 0` or `chunk_id >= chunk_count`.
pub fn uniform_hashed_keys_chunk(chunk_id: u64, chunk_count: u64) -> ops::RangeInclusive<H256> {
    assert!(chunk_count > 0, "`chunk_count` must be positive");
    assert!(
        chunk_id < chunk_count,
        "Chunk index {} exceeds count {}",
        chunk_id,
        chunk_count
    );

    let mut stride = U256::MAX / chunk_count;
    let stride_minus_one = if stride < U256::MAX {
        stride += U256::one();
        stride - 1
    } else {
        stride // `stride` is really 1 << 256 == U256::MAX + 1
    };

    let start = stride * chunk_id;
    let (mut end, is_overflow) = stride_minus_one.overflowing_add(start);
    if is_overflow {
        end = U256::MAX;
    }
    u256_to_h256(start)..=u256_to_h256(end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::h256_to_u256;

    #[test]
    fn chunking_is_correct() {
        for chunks_count in (2..10).chain([42, 256, 500, 1_001, 12_345]) {
            println!("Testing chunks_count={chunks_count}");
            let chunked_ranges: Vec<_> = (0..chunks_count)
                .map(|chunk_id| uniform_hashed_keys_chunk(chunk_id, chunks_count))
                .collect();

            assert_eq!(*chunked_ranges[0].start(), H256::zero());
            assert_eq!(
                *chunked_ranges.last().unwrap().end(),
                H256::repeat_byte(0xff)
            );
            for window in chunked_ranges.windows(2) {
                let [prev_chunk, next_chunk] = window else {
                    unreachable!();
                };
                assert_eq!(
                    h256_to_u256(*prev_chunk.end()) + 1,
                    h256_to_u256(*next_chunk.start())
                );
            }

            let chunk_sizes: Vec<_> = chunked_ranges
                .iter()
                .map(|chunk| h256_to_u256(*chunk.end()) - h256_to_u256(*chunk.start()) + 1)
                .collect();

            // Check that chunk sizes are roughly equal. Due to how chunks are constructed, the sizes
            // of all chunks except for the last one are the same, and the last chunk size may be slightly smaller;
            // the difference in sizes is lesser than the number of chunks.
            let min_chunk_size = chunk_sizes.iter().copied().min().unwrap();
            let max_chunk_size = chunk_sizes.iter().copied().max().unwrap();
            assert!(max_chunk_size - min_chunk_size < U256::from(chunks_count));
        }
    }
}
