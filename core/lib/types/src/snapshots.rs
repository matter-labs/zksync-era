use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllSnapshots {
    pub snapshots: Vec<SnapshotBasicMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotBasicMetadata {
    pub l1_batch_number: L1BatchNumber,
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFullMetadata {
    pub l1_batch_number: L1BatchNumber,
    pub generated_at: DateTime<Utc>,
    pub storage_logs_files: Vec<String>,
}

pub struct SnapshotInProgress {
    pub l1_batch_number: L1BatchNumber,
    pub last_processed_chunk: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct StorageLogsSnapshotKey {
    pub l1_batch_number: L1BatchNumber,
    pub chunk_id: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct StorageLogsSnapshot {
    pub l1_batch_number: L1BatchNumber, //TODO storage_logs list
}
