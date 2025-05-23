use std::{num::NonZeroU32, time::Duration};

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct PruningConfig {
    #[config(default)]
    pub enabled: bool,
    /// Number of L1 batches pruned at a time.
    #[config(default_t = NonZeroU32::new(10).unwrap())]
    pub chunk_size: NonZeroU32,
    /// Delta between soft- and hard-removing data from Postgres. Should be reasonably large (order of 60 seconds).
    /// The default value is 60 seconds.
    #[config(default_t = 1 * TimeUnit::Minutes)]
    pub removal_delay: Duration,
    /// If set, L1 batches will be pruned after the batch timestamp is this old (in seconds). Note that an L1 batch
    /// may be temporarily retained for other reasons; e.g., a batch cannot be pruned until it is executed on L1,
    /// which happens roughly 24 hours after its generation on the mainnet. Thus, in practice this value can specify
    /// the retention period greater than that implicitly imposed by other criteria (e.g., 7 or 30 days).
    /// If set to 0, L1 batches will not be retained based on their timestamp. The default value is 1 hour.
    #[config(default_t = 1 * TimeUnit::Hours)]
    pub data_retention: Duration,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> PruningConfig {
        PruningConfig {
            enabled: true,
            chunk_size: NonZeroU32::new(10).unwrap(),
            removal_delay: Duration::from_secs(60),
            data_retention: Duration::from_secs(3600),
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            EN_PRUNING_ENABLED=true
            EN_PRUNING_DATA_RETENTION_SEC=3600
            EN_PRUNING_CHUNK_SIZE=10
            EN_PRUNING_REMOVAL_DELAY_SEC=60
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("EN_PRUNING_");

        let config: PruningConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
            chunk_size: 10
            data_retention_sec: 3600
            enabled: true
            removal_delay_sec: 60
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: PruningConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
