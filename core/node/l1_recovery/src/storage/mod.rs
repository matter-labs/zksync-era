//! For the sake of backwards compatibility the column handles may not actually
//! be accurately named.
pub mod reconstruction;

pub mod snapshot;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zksync_basic_types::U256;

pub const INDEX_TO_KEY_MAP: &str = "index_to_key_map";
pub const KEY_TO_INDEX_MAP: &str = "key_to_index_map";
pub const METADATA: &str = "metadata";

pub mod reconstruction_columns {
    pub const LAST_REPEATED_KEY_INDEX: &str = "LAST_REPEATED_KEY_INDEX";
}

pub mod snapshot_columns {
    pub const STORAGE_LOGS: &str = "storage_logs";
    pub const FACTORY_DEPS: &str = "factory_deps";

    pub const LAST_REPEATED_KEY_INDEX: &str = "SNAPSHOT_LAST_REPEATED_KEY_INDEX";
    /// The latest l1 block number that was processed.
    pub const LATEST_L1_BLOCK: &str = "SNAPSHOT_LATEST_L1_BATCH";
    /// The latest l1 batch number that was processed. This is the batch number
    /// of the ZKSync transactions.
    pub const LATEST_L1_BATCH: &str = "SNAPSHOT_LATEST_L2_BATCH";
    /// The latest l2 block number that was processed.
    pub const LATEST_L2_BLOCK: &str = "SNAPSHOT_LATEST_L2_BLOCK";
}

// NOTE: This is moved here as a temporary measure to resolve a cyclic dependency issue.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum PackingType {
    Add(U256),
    Sub(U256),
    Transform(U256),
    NoCompression(U256),
}

#[derive(Error, Debug)]
pub enum DatabaseError {
    #[error("key not found")]
    NoSuchKey,
}
