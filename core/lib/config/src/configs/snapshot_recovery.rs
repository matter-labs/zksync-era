use std::num::NonZeroUsize;

use serde::Deserialize;
use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct SnapshotRecoveryConfig {
    pub enabled: bool,
    pub postgres_max_concurrency: Option<NonZeroUsize>,
    pub tree_chunk_size: Option<u64>,
    pub l1_batch: Option<L1BatchNumber>,
    pub tree_parallel_persistence_buffer: Option<NonZeroUsize>,
}
