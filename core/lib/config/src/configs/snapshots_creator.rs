use serde::Deserialize;
use zksync_basic_types::L1BatchNumber;

use crate::ObjectStoreConfig;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SnapshotsCreatorConfig {
    /// Version of snapshots to create.
    // Raw integer version is used because `SnapshotVersion` is defined in `zksync_types` crate.
    #[serde(default)]
    pub version: u16,
    /// L1 batch number to create the snapshot for. If not specified, a snapshot will be created
    /// for the current penultimate L1 batch.
    ///
    /// - If a snapshot with this L1 batch already exists and is complete, the creator will do nothing.
    /// - If a snapshot with this L1 batch exists and is incomplete, the creator will continue creating it,
    ///   regardless of whether the specified snapshot `version` matches.
    pub l1_batch_number: Option<L1BatchNumber>,
    #[serde(default = "SnapshotsCreatorConfig::storage_logs_chunk_size_default")]
    pub storage_logs_chunk_size: u64,
    #[serde(default = "SnapshotsCreatorConfig::concurrent_queries_count")]
    pub concurrent_queries_count: u32,
    pub object_store: Option<ObjectStoreConfig>,
}

impl SnapshotsCreatorConfig {
    const fn storage_logs_chunk_size_default() -> u64 {
        1_000_000
    }

    const fn concurrent_queries_count() -> u32 {
        25
    }
}
