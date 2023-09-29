// Built-in uses
use std::time::Duration;
// External uses
use serde::Deserialize;
// Local uses
use super::envy_load;

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
    pub fn from_env() -> Self {
        envy_load("eth_watch", "ETH_WATCH_")
    }

    /// Converts `self.eth_node_poll_interval` into `Duration`.
    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.eth_node_poll_interval)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ETHWatchConfig {
        ETHWatchConfig {
            confirmations_for_eth_event: Some(0),
            eth_node_poll_interval: 300,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            ETH_WATCH_CONFIRMATIONS_FOR_ETH_EVENT="0"
            ETH_WATCH_ETH_NODE_POLL_INTERVAL="300"
        "#;
        lock.set_env(config);

        let actual = ETHWatchConfig::from_env();
        assert_eq!(actual, expected_config());
    }

    /// Checks the correctness of the config helper methods.
    #[test]
    fn methods() {
        let config = expected_config();
        assert_eq!(
            config.poll_interval(),
            Duration::from_millis(config.eth_node_poll_interval)
        );
    }
}
