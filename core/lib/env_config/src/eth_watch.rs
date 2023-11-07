use zksync_config::ETHWatchConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ETHWatchConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("eth_watch", "ETH_WATCH_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

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

        let actual = ETHWatchConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
