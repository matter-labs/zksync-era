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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub metadata: SnapshotMetadata,
    pub miniblock_number: MiniblockNumber,
    pub chunks: Vec<SnapshotChunkMetadata>,
    pub last_l1_batch_with_metadata: L1BatchWithMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotChunkMetadata {
    pub key: SnapshotStorageKey,
    pub filepath: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStorageKey {
    pub l1_batch_number: L1BatchNumber,
    pub chunk_id: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotChunk {
    pub storage_logs: Vec<SnapshotStorageLog>,
    pub factory_deps: Vec<SnapshotFactoryDependency>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotStorageLog {
    pub key: StorageKey,
    pub value: StorageValue,
    pub miniblock_number: MiniblockNumber,
    pub l1_batch_number: L1BatchNumber,
    pub enumeration_index: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppliedSnapshotStatus {
    pub l1_batch_number: L1BatchNumber,
    pub is_finished: bool,
    pub last_finished_chunk_id: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFactoryDependency {
    pub bytecode_hash: H256,
    pub bytecode: Vec<u8>,
}
