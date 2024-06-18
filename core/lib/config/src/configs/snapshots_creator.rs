use serde::Deserialize;

use crate::ObjectStoreConfig;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SnapshotsCreatorConfig {
    /// Version of snapshots to create.
    // Raw integer version is used because `SnapshotVersion` is defined in `zksync_types` crate.
    #[serde(default)]
    pub version: u16,

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
