use serde::{Deserialize, Serialize};
use zksync_basic_types::{L1BatchNumber, MiniblockNumber};

use crate::{commitment::L1BatchWithMetadata, Bytes, StorageKey, StorageValue};

/// Information about all snapshots persisted by the node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllSnapshots {
    /// L1 batch numbers for complete snapshots. Ordered by descending number (i.e., 0th element
    /// corresponds to the newest snapshot).
    pub snapshots_l1_batch_numbers: Vec<L1BatchNumber>,
    /// Same as `snapshots_l1_batch_numbers`, but for incomplete snapshots (ones in which some
    /// storage log chunks may be missing).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub incomplete_snapshots_l1_batch_numbers: Vec<L1BatchNumber>,
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
/// Contains all data not contained in factory deps / storage logs files to perform restore process.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHeader {
    pub l1_batch_number: L1BatchNumber,
    pub miniblock_number: MiniblockNumber,
    pub is_complete: bool,
    /// Ordered by chunk IDs.
    pub storage_logs_chunks: Vec<SnapshotStorageLogsChunkMetadata>,
    pub factory_deps_filepath: String,
    pub last_l1_batch_with_metadata: L1BatchWithMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStorageLogsChunkMetadata {
    pub chunk_id: u64,
    // can be either be a file available under http(s) or local filesystem path
    pub filepath: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStorageLogsStorageKey {
    pub l1_batch_number: L1BatchNumber,
    pub chunk_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStorageLogsChunk {
    pub storage_logs: Vec<SnapshotStorageLog>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStorageLog {
    pub key: StorageKey,
    pub value: StorageValue,
    pub l1_batch_number_of_initial_write: L1BatchNumber,
    pub enumeration_index: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFactoryDependencies {
    pub factory_deps: Vec<SnapshotFactoryDependency>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFactoryDependency {
    pub bytecode: Bytes,
}
