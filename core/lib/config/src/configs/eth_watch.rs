use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

use crate::utils::Fallback;

/// Configuration for the Ethereum watch crate.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct EthWatchConfig {
    /// Amount of confirmations for the priority operation to be processed.
    /// If not specified operation will be processed once its block is finalized.
    #[config(default)]
    pub confirmations_for_eth_event: Option<u64>,
    /// How often we want to poll the Ethereum node.
    #[config(default_t = Duration::from_secs(1), with = Fallback(TimeUnit::Millis))]
    pub eth_node_poll_interval: Duration,
    /// How many L1 blocks to look back for the priority operations.
    #[config(default_t = 50_000)]
    pub event_expiration_blocks: u64,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Yaml};

    use super::*;

    fn expected_config() -> EthWatchConfig {
        EthWatchConfig {
            confirmations_for_eth_event: Some(5),
            eth_node_poll_interval: Duration::from_secs(3),
            event_expiration_blocks: 10_000,
        }
    }

    #[test]
    fn from_yaml() {
        let yaml = r#"
          confirmations_for_eth_event: 5
          eth_node_poll_interval: 3000
          event_expiration_blocks: 10000
        "#;
        let yaml = serde_yaml::from_str(yaml).unwrap();
        let yaml = Yaml::new("test.yml", yaml).unwrap();

        let config: EthWatchConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn from_yaml_with_suffixed_duration() {
        let yaml = r#"
          confirmations_for_eth_event: 5
          eth_node_poll_interval_sec: 3
          event_expiration_blocks: 10000
        "#;
        let yaml = serde_yaml::from_str(yaml).unwrap();
        let yaml = Yaml::new("test.yml", yaml).unwrap();

        let config: EthWatchConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn from_yaml_with_idiomatic_duration() {
        let yaml = r#"
          confirmations_for_eth_event: 5
          eth_node_poll_interval: 3 sec
          event_expiration_blocks: 10000
        "#;
        let yaml = serde_yaml::from_str(yaml).unwrap();
        let yaml = Yaml::new("test.yml", yaml).unwrap();

        let config: EthWatchConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
