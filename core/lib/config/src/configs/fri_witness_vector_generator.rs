use std::time::Duration;

use serde::Deserialize;

use super::envy_load;

/// Configuration for the witness vector generator
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriWitnessVectorGeneratorConfig {
    /// Max time before an `reserved` prover instance in considered as `available`
    pub max_prover_reservation_duration_in_secs: u16,
    /// Max time to wait to get a free prover instance
    pub prover_instance_wait_timeout_in_secs: u16,
    // Time to wait between 2 consecutive poll to get new prover instance.
    pub prover_instance_poll_time_in_milli_secs: u16,

    /// Configurations for prometheus
    pub prometheus_listener_port: u16,
    pub prometheus_pushgateway_url: String,
    pub prometheus_push_interval_ms: Option<u64>,

    // specialized group id for this witness vector generator.
    // witness vector generator running the same (circuit id, round) shall have same group id.
    pub specialized_group_id: u8,
}

impl FriWitnessVectorGeneratorConfig {
    pub fn from_env() -> Self {
        envy_load(
            "fri_witness_vector_generator",
            "FRI_WITNESS_VECTOR_GENERATOR_",
        )
    }

    pub fn prover_instance_wait_timeout(&self) -> Duration {
        Duration::from_secs(self.prover_instance_wait_timeout_in_secs as u64)
    }

    pub fn prover_instance_poll_time(&self) -> Duration {
        Duration::from_millis(self.prover_instance_poll_time_in_milli_secs as u64)
    }

    pub fn max_prover_reservation_duration(&self) -> Duration {
        Duration::from_secs(self.max_prover_reservation_duration_in_secs as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> FriWitnessVectorGeneratorConfig {
        FriWitnessVectorGeneratorConfig {
            max_prover_reservation_duration_in_secs: 1000u16,
            prover_instance_wait_timeout_in_secs: 1000u16,
            prover_instance_poll_time_in_milli_secs: 250u16,
            prometheus_listener_port: 3316,
            prometheus_pushgateway_url: "http://127.0.0.1:9091".to_string(),
            prometheus_push_interval_ms: Some(100),
            specialized_group_id: 1,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            FRI_WITNESS_VECTOR_GENERATOR_MAX_PROVER_RESERVATION_DURATION_IN_SECS=1000
            FRI_WITNESS_VECTOR_GENERATOR_PROVER_INSTANCE_WAIT_TIMEOUT_IN_SECS=1000
            FRI_WITNESS_VECTOR_GENERATOR_PROVER_INSTANCE_POLL_TIME_IN_MILLI_SECS=250
            FRI_WITNESS_VECTOR_GENERATOR_PROMETHEUS_LISTENER_PORT=3316
            FRI_WITNESS_VECTOR_GENERATOR_PROMETHEUS_PUSHGATEWAY_URL="http://127.0.0.1:9091"
            FRI_WITNESS_VECTOR_GENERATOR_PROMETHEUS_PUSH_INTERVAL_MS=100
            FRI_WITNESS_VECTOR_GENERATOR_SPECIALIZED_GROUP_ID=1
        "#;
        lock.set_env(config);

        let actual = FriWitnessVectorGeneratorConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
