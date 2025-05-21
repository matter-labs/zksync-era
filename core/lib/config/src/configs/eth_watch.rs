use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

/// Configuration for the Ethereum watch crate.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct EthWatchConfig {
    /// Amount of confirmations for the priority operation to be processed.
    /// If not specified operation will be processed once its block is finalized.
    #[config(default)]
    pub confirmations_for_eth_event: Option<u64>,
    /// How often we want to poll the Ethereum node.
    #[config(default_t = Duration::from_secs(1), with = TimeUnit::Millis)]
    pub eth_node_poll_interval: Duration,
}
