use std::num::NonZeroU64;

use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PruningConfig {
    pub enabled: bool,
    /// Number of L1 batches pruned at a time.
    pub chunk_size: Option<u32>,
    /// Delta between soft- and hard-removing data from Postgres. Should be reasonably large (order of 60 seconds).
    /// The default value is 60 seconds.
    pub removal_delay_sec: Option<NonZeroU64>,
    /// If set, L1 batches will be pruned after the batch timestamp is this old (in seconds). Note that an L1 batch
    /// may be temporarily retained for other reasons; e.g., a batch cannot be pruned until it is executed on L1,
    /// which happens roughly 24 hours after its generation on the mainnet. Thus, in practice this value can specify
    /// the retention period greater than that implicitly imposed by other criteria (e.g., 7 or 30 days).
    /// If set to 0, L1 batches will not be retained based on their timestamp. The default value is 1 hour.
    pub data_retention_sec: Option<u64>,
}
