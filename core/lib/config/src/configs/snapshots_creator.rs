use smart_config::{
    de::{Optional, Serde},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::L1BatchNumber;

use crate::ObjectStoreConfig;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct SnapshotsCreatorConfig {
    /// Version of snapshots to create.
    // Raw integer version is used because `SnapshotVersion` is defined in `zksync_types` crate.
    #[config(default)]
    pub version: u16,
    /// L1 batch number to create the snapshot for. If not specified, a snapshot will be created
    /// for the current penultimate L1 batch.
    ///
    /// - If a snapshot with this L1 batch already exists and is complete, the creator will do nothing.
    /// - If a snapshot with this L1 batch exists and is incomplete, the creator will continue creating it,
    ///   regardless of whether the specified snapshot `version` matches.
    #[config(with = Optional(Serde![int]))]
    pub l1_batch_number: Option<L1BatchNumber>,
    #[config(default_t = 1_000_000)]
    pub storage_logs_chunk_size: u64,
    #[config(default_t = 25)]
    pub concurrent_queries_count: u32,
    #[config(nest)]
    pub object_store: ObjectStoreConfig,
}
