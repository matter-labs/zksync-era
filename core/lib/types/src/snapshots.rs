use crate::commitment::L1BatchWithMetadata;
use crate::{StorageKey, StorageValue, H256};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zksync_basic_types::{L1BatchNumber, MiniblockNumber};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllSnapshots {
    pub snapshots: Vec<SnapshotMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMetadata {
    pub l1_batch_number: L1BatchNumber,
    pub generated_at: DateTime<Utc>,
    pub factory_deps_filepath: String,
    pub storage_logs_filepaths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHeader {
    pub l1_batch_number: L1BatchNumber,
    pub miniblock_number: MiniblockNumber,
    pub storage_logs_chunks: Vec<SnapshotStorageLogsChunkMetadata>,
    pub factory_deps_filepath: String,
    pub last_l1_batch_with_metadata: L1BatchWithMetadata,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStorageLogsChunkMetadata {
    pub chunk_id: u64,
    pub filepath: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFactoryDependency {
    pub bytecode_hash: H256,
    pub bytecode: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedSnapshotStatus {
    pub l1_batch_number: L1BatchNumber,
    pub is_finished: bool,
    pub last_finished_chunk_id: Option<u64>,
}
