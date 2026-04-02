use std::time::Duration;

use smart_config::{de::Serde, metadata::TimeUnit, DescribeConfig, DeserializeConfig};
use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct AirbenderProofDataHandlerConfig {
    pub http_port: u16,
    /// All batches before this one are considered to be processed.
    #[config(default, with = Serde![int])]
    pub first_processed_batch: L1BatchNumber,
    /// Timeout for retrying proof generation if it previously failed or if it was picked by a
    /// prover but the proof was not submitted within that time. Should be longer than the
    /// expected proving time to avoid duplicate work.
    #[config(default_t = 60 * TimeUnit::Minutes)]
    pub proof_generation_timeout: Duration,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Yaml};

    use super::*;

    fn expected_config() -> AirbenderProofDataHandlerConfig {
        AirbenderProofDataHandlerConfig {
            http_port: 4320,
            first_processed_batch: L1BatchNumber(123),
            proof_generation_timeout: Duration::from_secs(90),
        }
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          http_port: 4320
          first_processed_batch: 123
          proof_generation_timeout_in_secs: 90
        "#;
        let yaml = serde_yaml::from_str(yaml).unwrap();
        let yaml = Yaml::new("test.yml", yaml).unwrap();

        let config: AirbenderProofDataHandlerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_idiomatic_yaml() {
        let yaml = r#"
          http_port: 4320
          first_processed_batch: 123
          proof_generation_timeout: 90s
        "#;
        let yaml = serde_yaml::from_str(yaml).unwrap();
        let yaml = Yaml::new("test.yml", yaml).unwrap();

        let config: AirbenderProofDataHandlerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
