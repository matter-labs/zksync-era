use std::{num::NonZeroU64, time::Duration};

use smart_config::{DescribeConfig, DeserializeConfig};

/// Configuration for node synchronization
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct NodeSyncConfig {
    /// Interval between batch transaction updates
    #[config(default_t = Duration::from_millis(5000))]
    pub batch_transaction_updater_interval: Duration,
    /// Maximum number of transactions to process in a single batch
    #[config(default_t = NonZeroU64::new(10_000).unwrap())]
    pub batch_transaction_updater_batch_size: NonZeroU64,
    /// Toggle for disabling seal criteria validation in case of some issues / forced proved batches
    #[config(default_t = true)]
    pub validate_seal_criteria: bool,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> NodeSyncConfig {
        NodeSyncConfig {
            batch_transaction_updater_interval: Duration::from_secs(2),
            batch_transaction_updater_batch_size: NonZeroU64::new(100).unwrap(),
            validate_seal_criteria: false,
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            NODE_SYNC_BATCH_TRANSACTION_UPDATER_INTERVAL=2sec
            NODE_SYNC_BATCH_TRANSACTION_UPDATER_BATCH_SIZE=100
            NODE_SYNC_VALIDATE_SEAL_CRITERIA=false
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("NODE_SYNC_");

        let config: NodeSyncConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          batch_transaction_updater_interval: 2sec
          batch_transaction_updater_batch_size: 100
          validate_seal_criteria: false
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: NodeSyncConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
