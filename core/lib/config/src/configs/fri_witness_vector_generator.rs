use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

/// Configuration for the witness vector generator
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct FriWitnessVectorGeneratorConfig {
    /// Max time before an `reserved` prover instance in considered as `available`
    #[config(default_t = Duration::from_secs(1_000), with = TimeUnit::Seconds)]
    pub max_prover_reservation_duration_in_secs: Duration,
    /// Max time to wait to get a free prover instance
    #[config(default_t = Duration::from_secs(200), with = TimeUnit::Seconds)]
    pub prover_instance_wait_timeout_in_secs: Duration,
    /// Time to wait between 2 consecutive poll to get new prover instance.
    #[config(default_t = Duration::from_millis(250), with = TimeUnit::Millis)]
    pub prover_instance_poll_time_in_milli_secs: Duration,

    // Configurations for prometheus
    pub prometheus_listener_port: u16,

    /// Specialized group id for this witness vector generator.
    /// Witness vector generator running the same (circuit id, round) shall have same group id.
    pub specialized_group_id: u8,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> FriWitnessVectorGeneratorConfig {
        FriWitnessVectorGeneratorConfig {
            max_prover_reservation_duration_in_secs: Duration::from_secs(1000),
            prover_instance_wait_timeout_in_secs: Duration::from_secs(1000),
            prover_instance_poll_time_in_milli_secs: Duration::from_millis(250),
            prometheus_listener_port: 3316,
            specialized_group_id: 1,
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            FRI_WITNESS_VECTOR_GENERATOR_MAX_PROVER_RESERVATION_DURATION_IN_SECS=1000
            FRI_WITNESS_VECTOR_GENERATOR_PROVER_INSTANCE_WAIT_TIMEOUT_IN_SECS=1000
            FRI_WITNESS_VECTOR_GENERATOR_PROVER_INSTANCE_POLL_TIME_IN_MILLI_SECS=250
            FRI_WITNESS_VECTOR_GENERATOR_PROMETHEUS_LISTENER_PORT=3316
            FRI_WITNESS_VECTOR_GENERATOR_SPECIALIZED_GROUP_ID=1
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("FRI_WITNESS_VECTOR_GENERATOR_");

        let config: FriWitnessVectorGeneratorConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          max_prover_reservation_duration_in_secs: 1000
          prover_instance_wait_timeout_in_secs: 1000
          prover_instance_poll_time_in_milli_secs: 250
          prometheus_listener_port: 3316
          specialized_group_id: 1
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: FriWitnessVectorGeneratorConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
