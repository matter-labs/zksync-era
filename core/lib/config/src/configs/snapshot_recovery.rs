use std::num::NonZeroUsize;

use serde::Deserialize;
use zksync_basic_types::L1BatchNumber;

use crate::ObjectStoreConfig;

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct TreeRecoveryConfig {
    /// Approximate chunk size (measured in the number of entries) to recover in a single iteration.
    /// Reasonable values are order of 100,000 (meaning an iteration takes several seconds).
    ///
    /// **Important.** This value cannot be changed in the middle of tree recovery (i.e., if a node is stopped in the middle
    /// of recovery and then restarted with a different config).
    pub chunk_size: Option<u64>,
    /// Buffer capacity for parallel persistence operations. Should be reasonably small since larger buffer means more RAM usage;
    /// buffer elements are persisted tree chunks. OTOH, small buffer can lead to persistence parallelization being inefficient.
    ///
    /// If not set, parallel persistence will be disabled.
    pub parallel_persistence_buffer: Option<NonZeroUsize>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct PostgresRecoveryConfig {
    /// Maximum concurrency factor for the concurrent parts of snapshot recovery for Postgres. It may be useful to
    /// reduce this factor to about 5 if snapshot recovery overloads I/O capacity of the node. Conversely,
    /// if I/O capacity of your infra is high, you may increase concurrency to speed up Postgres recovery.
    pub max_concurrency: Option<NonZeroUsize>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SnapshotRecoveryConfig {
    /// Enables application-level snapshot recovery. Required to start a node that was recovered from a snapshot,
    /// or to initialize a node from a snapshot. Has no effect if a node that was initialized from a Postgres dump
    /// or was synced from genesis.
    ///
    /// This is an experimental and incomplete feature; do not use unless you know what you're doing.
    pub enabled: bool,
    /// L1 batch number of the snapshot to use during recovery. Specifying this parameter is mostly useful for testing.
    pub l1_batch: Option<L1BatchNumber>,
    /// Enables dropping storage key preimages when recovering storage logs from a snapshot with version 0.
    /// This is a temporary flag that will eventually be removed together with version 0 snapshot support.
    #[serde(default)]
    pub drop_storage_key_preimages: bool,
    pub tree: TreeRecoveryConfig,
    pub postgres: PostgresRecoveryConfig,
    pub object_store: Option<ObjectStoreConfig>,
}
