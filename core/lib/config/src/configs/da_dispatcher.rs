use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct DADispatcherConfig {
    /// The interval between the `da_dispatcher's` iterations.
    #[config(default_t = Duration::from_secs(5), with = TimeUnit::Millis)]
    pub polling_interval_ms: Duration,
    /// The maximum number of rows to query from the database in a single query.
    #[config(default_t = 100)]
    pub max_rows_to_dispatch: u32,
    /// The maximum number of retries for the dispatch of a blob.
    #[config(default_t = 5)]
    pub max_retries: u16,
    /// Use dummy value as inclusion proof instead of getting it from the client.
    // TODO: run a verification task to check if the L1 contract expects the inclusion proofs to
    //   avoid the scenario where contracts expect real proofs, and server is using dummy proofs.
    #[config(default)]
    pub use_dummy_inclusion_data: bool,
}
