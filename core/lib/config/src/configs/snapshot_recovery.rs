use std::num::NonZeroUsize;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SnapshotRecoveryConfig {
    pub enabled: bool,
    pub postgres_max_concurrency: Option<NonZeroUsize>,
    pub tree_chunk_size: Option<u64>,
}
