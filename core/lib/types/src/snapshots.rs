use std::{convert::TryFrom, ops};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{AccountTreeId, L1BatchNumber, MiniblockNumber, H256};
use zksync_protobuf::{required, ProtoFmt};
use zksync_utils::u256_to_h256;

use crate::{commitment::L1BatchWithMetadata, Bytes, StorageKey, StorageValue, H160, U256};

/// Information about all snapshots persisted by the node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllSnapshots {
    /// L1 batch numbers for complete snapshots. Ordered by descending number (i.e., 0th element
    /// corresponds to the newest snapshot).
    pub snapshots_l1_batch_numbers: Vec<L1BatchNumber>,
}

/// Storage snapshot metadata. Used in DAL to fetch certain snapshot data.
#[derive(Debug, Clone)]
pub struct SnapshotMetadata {
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
    pub l1_batch_number: L1BatchNumber,
    pub miniblock_number: MiniblockNumber,
    /// Ordered by chunk IDs.
    pub storage_logs_chunks: Vec<SnapshotStorageLogsChunkMetadata>,
    pub factory_deps_filepath: String,
    pub last_l1_batch_with_metadata: L1BatchWithMetadata,
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
pub struct SnapshotStorageLogsChunk {
    pub storage_logs: Vec<SnapshotStorageLog>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotStorageLog {
    pub key: StorageKey,
    pub value: StorageValue,
    pub l1_batch_number_of_initial_write: L1BatchNumber,
    pub enumeration_index: u64,
}

#[derive(Debug, PartialEq)]
pub struct SnapshotFactoryDependencies {
    pub factory_deps: Vec<SnapshotFactoryDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotFactoryDependency {
    pub bytecode: Bytes,
}

impl ProtoFmt for SnapshotFactoryDependency {
    type Proto = crate::proto::SnapshotFactoryDependency;

    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        Ok(Self {
            bytecode: Bytes(required(&r.bytecode).context("bytecode")?.clone()),
        })
    }
    fn build(&self) -> Self::Proto {
        Self::Proto {
            bytecode: Some(self.bytecode.0.as_slice().into()),
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
        Ok(Self {
            key: StorageKey::new(
                AccountTreeId::new(
                    required(&r.account_address)
                        .and_then(|bytes| Ok(<[u8; 20]>::try_from(bytes.as_slice())?.into()))
                        .context("account_address")?,
                ),
                required(&r.storage_key)
                    .and_then(|bytes| Ok(<[u8; 32]>::try_from(bytes.as_slice())?.into()))
                    .context("storage_key")?,
            ),
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
            account_address: Some(self.key.address().as_bytes().into()),
            storage_key: Some(self.key.key().as_bytes().into()),
            storage_value: Some(self.value.as_bytes().into()),
            l1_batch_number_of_initial_write: Some(self.l1_batch_number_of_initial_write.0),
            enumeration_index: Some(self.enumeration_index),
        }
    }
}

impl ProtoFmt for SnapshotStorageLogsChunk {
    type Proto = crate::proto::SnapshotStorageLogsChunk;

    fn read(r: &Self::Proto) -> anyhow::Result<Self> {
        let mut storage_logs = Vec::with_capacity(r.storage_logs.len());
        for (i, storage_log) in r.storage_logs.iter().enumerate() {
            storage_logs.push(
                SnapshotStorageLog::read(storage_log)
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
                .map(SnapshotStorageLog::build)
                .collect(),
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct SnapshotRecoveryStatus {
    pub l1_batch_number: L1BatchNumber,
    pub l1_batch_root_hash: H256,
    pub miniblock_number: MiniblockNumber,
    pub miniblock_root_hash: H256,
    pub storage_logs_chunks_processed: Vec<bool>,
}

// Used only in tests
#[derive(Debug, PartialEq)]
pub struct InitialWriteDbRow {
    pub hashed_key: H256,
    pub l1_batch_number: L1BatchNumber,
    pub index: u64,
}

// Used only in tests
#[derive(Debug, PartialEq)]
pub struct StorageLogDbRow {
    pub hashed_key: H256,
    pub address: H160,
    pub key: H256,
    pub value: H256,
    pub operation_number: u64,
    pub tx_hash: H256,
    pub miniblock_number: MiniblockNumber,
}

/// Returns a chunk of `hashed_keys` with 0-based index `chunk_id` among `count`. Chunks do not intersect and jointly cover
/// the entire `hashed_key` space. If `hashed_key`s are uniformly distributed (which is the case), the returned ranges
/// are expected to contain the same number of entries.
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
    use zksync_utils::h256_to_u256;

    use super::*;

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
