use std::time::Duration;

use serde::{Deserialize, Serialize};

const DEFAULT_EVENT_EXPIRATION_BLOCKS: u64 = 50_000;

/// Configuration for the Ethereum watch crate.
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct EthWatchConfig {
    /// Amount of confirmations for the priority operation to be processed.
    /// If not specified operation will be processed once its block is finalized.
    pub confirmations_for_eth_event: Option<u64>,
    /// How often we want to poll the Ethereum node.
    /// Value in milliseconds.
    pub eth_node_poll_interval: u64,
    /// How many L1 blocks to look back for the priority operations.
    #[serde(default = "EthWatchConfig::default_event_expiration_blocks")]
    pub event_expiration_blocks: u64,
}

impl EthWatchConfig {
    /// Converts `self.eth_node_poll_interval` into `Duration`.
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.eth_node_poll_interval)
    }

    pub fn default_event_expiration_blocks() -> u64 {
        DEFAULT_EVENT_EXPIRATION_BLOCKS
    }
}
