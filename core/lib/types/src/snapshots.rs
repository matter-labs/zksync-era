use std::{fmt, str::FromStr};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use zksync_basic_types::{L1BatchNumber, MiniblockNumber};

use crate::{commitment::L1BatchWithMetadata, Bytes, StorageKey, StorageValue};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AllSnapshots {
    pub snapshots_l1_batch_numbers: Vec<L1BatchNumber>,
}

/// Storage snapshot metadata. Used in DAL to fetch certain snapshot data.
#[derive(Debug, Clone)]
pub struct SnapshotMetadata {
    pub l1_batch_number: L1BatchNumber,
    pub factory_deps_filepath: String,
    pub storage_logs_filepaths: Vec<Option<String>>,
}

impl SnapshotMetadata {
    pub fn is_complete(&self) -> bool {
        self.storage_logs_filepaths.iter().all(Option::is_some)
    }
}

//contains all data not contained in factory_deps/storage_logs files to perform restore process
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotHeader {
    pub l1_batch_number: L1BatchNumber,
    pub miniblock_number: MiniblockNumber,
    //ordered by chunk ids
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

impl fmt::Display for SnapshotStorageLogsStorageKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "snapshot_l1_batch_{}_storage_logs_part_{:0>4}.json.gzip",
            self.l1_batch_number, self.chunk_id
        )
    }
}

impl FromStr for SnapshotStorageLogsStorageKey {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let s = s
            .strip_prefix("snapshot_l1_batch_")
            .context("missing `snapshot_l1_batch_` prefix")?;
        let underscore_pos = s.find('_').context("missing '_' char")?;
        let (l1_batch_str, s) = s.split_at(underscore_pos);
        let l1_batch_number = L1BatchNumber(l1_batch_str.parse().context("invalid L1 batch")?);

        let s = s
            .strip_prefix("_storage_logs_part_")
            .context("missing `_storage_logs_part_`")?;
        let s = s
            .strip_suffix(".json.gzip")
            .context("missing `.json.gzip` suffix")?;
        let chunk_id = s.parse().context("invalid chunk ID")?;

        Ok(Self {
            l1_batch_number,
            chunk_id,
        })
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn displaying_and_parsing_storage_logs_key() {
        let key = SnapshotStorageLogsStorageKey {
            l1_batch_number: L1BatchNumber(12345),
            chunk_id: 42,
        };
        let key_str = key.to_string();
        assert_eq!(
            key_str,
            "snapshot_l1_batch_12345_storage_logs_part_0042.json.gzip"
        );

        let parsed_key: SnapshotStorageLogsStorageKey = key_str.parse().unwrap();
        assert_eq!(parsed_key, key);
    }
}
