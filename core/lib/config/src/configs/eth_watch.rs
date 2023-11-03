// Built-in uses
use std::time::Duration;
// External uses
use serde::Deserialize;

/// Configuration for the Ethereum sender crate.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ETHWatchConfig {
    /// Amount of confirmations for the priority operation to be processed.
    /// If not specified operation will be processed once its block is finalized.
    pub confirmations_for_eth_event: Option<u64>,
    /// How often we want to poll the Ethereum node.
    /// Value in milliseconds.
    pub eth_node_poll_interval: u64,
}

impl ETHWatchConfig {
    /// Converts `self.eth_node_poll_interval` into `Duration`.
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.eth_node_poll_interval)
    }
}
