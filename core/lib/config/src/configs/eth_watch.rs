use std::time::Duration;

use serde::{Deserialize, Serialize};

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
    #[serde(default = "EthWatchConfig::default_state_keeper_db_block_cache_capacity_mb")]
    pub priotity_tx_expiration_blocks: u64,
}

impl EthWatchConfig {
    /// Converts `self.eth_node_poll_interval` into `Duration`.
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.eth_node_poll_interval)
    }

    pub fn default_priotity_tx_expiration_blocks() -> u64 {
        zksync_system_constants::PRIORITY_EXPIRATION
    }
}
