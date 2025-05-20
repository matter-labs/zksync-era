use std::time::Duration;

use smart_config::{de::Serde, metadata::TimeUnit, DescribeConfig, DeserializeConfig};
use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct TeeProofDataHandlerConfig {
    pub http_port: u16,
    /// All batches before this one are considered to be processed.
    #[config(default, with = Serde![int])]
    pub first_processed_batch: L1BatchNumber,
    /// Timeout in seconds for retrying the preparation of input for TEE proof generation if it
    /// previously failed (e.g., due to a transient network issue) or if it was picked by a TEE
    /// prover but the TEE proof was not submitted within that time.
    #[config(default_t = 1 * TimeUnit::Minutes, with = TimeUnit::Seconds)]
    pub proof_generation_timeout_in_secs: Duration,
    /// Timeout in hours after which a batch will be permanently ignored if repeated retries failed.
    #[config(default_t = 10 * TimeUnit::Days, with = TimeUnit::Hours)]
    pub batch_permanently_ignored_timeout_in_hours: Duration,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Yaml};

    use super::*;

    fn expected_config() -> TeeProofDataHandlerConfig {
        TeeProofDataHandlerConfig {
            http_port: 4320,
            first_processed_batch: L1BatchNumber(123),
            proof_generation_timeout_in_secs: Duration::from_secs(90),
            batch_permanently_ignored_timeout_in_hours: 5 * TimeUnit::Days,
        }
    }

    #[test]
    fn tee_proof_data_handler_config_from_yaml() {
        let yaml = r#"
          http_port: 4320
          first_processed_batch: 123
          proof_generation_timeout_in_secs: 90
          batch_permanently_ignored_timeout_in_hours: 120
        "#;
        let yaml = serde_yaml::from_str(yaml).unwrap();
        let yaml = Yaml::new("test.yml", yaml).unwrap();

        let config: TeeProofDataHandlerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
