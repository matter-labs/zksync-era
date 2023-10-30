use crate::commitment::L1BatchWithMetadata;
use crate::{StorageKey, StorageValue, H256};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zksync_basic_types::{L1BatchNumber, MiniblockNumber};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllSnapshots {
    pub snapshots: Vec<SnapshotBasicMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotBasicMetadata {
    pub l1_batch_number: L1BatchNumber,
    pub miniblock_number: MiniblockNumber,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotsWithFiles {
    pub metadata: SnapshotBasicMetadata,
    pub storage_logs_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFullInfo {
    pub metadata: SnapshotBasicMetadata,
    pub storage_logs_files: Vec<String>,
    pub last_l1_batch_with_metadata: L1BatchWithMetadata,
}

#[derive(Debug, Clone, Copy)]
pub struct SnapshotStorageKey {
    pub l1_batch_number: L1BatchNumber,
    pub chunk_id: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotChunk {
    pub storage_logs: Vec<SingleStorageLogSnapshot>,
    pub factory_deps: Vec<FactoryDependency>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SingleStorageLogSnapshot {
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
    pub miniblock_number: MiniblockNumber,
    pub is_finished: bool,
    pub last_finished_chunk_id: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FactoryDependency {
    pub bytecode_hash: H256,
    pub bytecode: Vec<u8>,
}
