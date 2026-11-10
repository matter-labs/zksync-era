use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

/// Config of the Airbender read-proofs producer. Which batches it serves follows
/// `airbender_proof_data_handler.first_processed_batch` and `max_proving_attempts`, so that it
/// works on exactly the batches the handler can hand out.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct AirbenderReadProofsProducerConfig {
    /// Poll interval while no batch needs proofs or the tree is behind.
    #[config(default_t = 10 * TimeUnit::Seconds)]
    pub poll_interval: Duration,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Yaml};

    use super::*;

    fn expected_config() -> AirbenderReadProofsProducerConfig {
        AirbenderReadProofsProducerConfig {
            poll_interval: Duration::from_secs(15),
        }
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          poll_interval_in_secs: 15
        "#;
        let yaml = serde_yaml::from_str(yaml).unwrap();
        let yaml = Yaml::new("test.yml", yaml).unwrap();

        let config: AirbenderReadProofsProducerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_idiomatic_yaml() {
        let yaml = r#"
          poll_interval: 15s
        "#;
        let yaml = serde_yaml::from_str(yaml).unwrap();
        let yaml = Yaml::new("test.yml", yaml).unwrap();

        let config: AirbenderReadProofsProducerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
