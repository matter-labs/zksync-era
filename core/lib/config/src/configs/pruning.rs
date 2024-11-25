use std::{num::NonZeroU32, time::Duration};

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct PruningConfig {
    #[config(default)]
    pub enabled: bool,
    /// Number of L1 batches pruned at a time.
    #[config(default_t = NonZeroU32::new(10).unwrap())]
    pub chunk_size: NonZeroU32,
    /// Delta between soft- and hard-removing data from Postgres. Should be reasonably large (order of 60 seconds).
    /// The default value is 60 seconds.
    #[config(default_t = Duration::from_secs(60), with = TimeUnit::Seconds)]
    pub removal_delay_sec: Duration,
    /// If set, L1 batches will be pruned after the batch timestamp is this old (in seconds). Note that an L1 batch
    /// may be temporarily retained for other reasons; e.g., a batch cannot be pruned until it is executed on L1,
    /// which happens roughly 24 hours after its generation on the mainnet. Thus, in practice this value can specify
    /// the retention period greater than that implicitly imposed by other criteria (e.g., 7 or 30 days).
    /// If set to 0, L1 batches will not be retained based on their timestamp. The default value is 1 hour.
    #[config(default_t = Duration::from_secs(3_600), with = TimeUnit::Seconds)]
    pub data_retention_sec: Duration,
}
