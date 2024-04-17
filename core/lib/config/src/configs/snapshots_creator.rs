use serde::Deserialize;

use crate::ObjectStoreConfig;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SnapshotsCreatorConfig {
    #[serde(default = "snapshots_creator_storage_logs_chunk_size_default")]
    pub storage_logs_chunk_size: u64,

    #[serde(default = "snapshots_creator_concurrent_queries_count")]
    pub concurrent_queries_count: u32,
    pub object_store: Option<ObjectStoreConfig>,
}

fn snapshots_creator_storage_logs_chunk_size_default() -> u64 {
    1_000_000
}

fn snapshots_creator_concurrent_queries_count() -> u32 {
    25
}
