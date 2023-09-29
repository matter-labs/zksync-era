use std::time::Duration;

use serde::Deserialize;

use super::envy_load;

/// Configuration for the witness generation
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct CircuitSynthesizerConfig {
    /// Max time for circuit to be synthesized
    pub generation_timeout_in_secs: u16,
    /// Max attempts for synthesizing circuit
    pub max_attempts: u32,
    /// Max time before an `reserved` prover instance in considered as `available`
    pub gpu_prover_queue_timeout_in_secs: u16,
    /// Max time to wait to get a free prover instance
    pub prover_instance_wait_timeout_in_secs: u16,
    // Time to wait between 2 consecutive poll to get new prover instance.
    pub prover_instance_poll_time_in_milli_secs: u16,
    /// Configurations for prometheus
    pub prometheus_listener_port: u16,
    pub prometheus_pushgateway_url: String,
    pub prometheus_push_interval_ms: Option<u64>,
    // Group id for this synthesizer, synthesizer running the same circuit types shall have same group id.
    pub prover_group_id: u8,
}

impl CircuitSynthesizerConfig {
    pub fn from_env() -> Self {
        envy_load("circuit_synthesizer", "CIRCUIT_SYNTHESIZER_")
    }

    pub fn generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_in_secs as u64)
    }

    pub fn prover_instance_wait_timeout(&self) -> Duration {
        Duration::from_secs(self.prover_instance_wait_timeout_in_secs as u64)
    }

    pub fn gpu_prover_queue_timeout(&self) -> Duration {
        Duration::from_secs(self.gpu_prover_queue_timeout_in_secs as u64)
    }

    pub fn prover_instance_poll_time(&self) -> Duration {
        Duration::from_millis(self.prover_instance_poll_time_in_milli_secs as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> CircuitSynthesizerConfig {
        CircuitSynthesizerConfig {
            generation_timeout_in_secs: 1000u16,
            max_attempts: 2,
            gpu_prover_queue_timeout_in_secs: 1000u16,
            prover_instance_wait_timeout_in_secs: 1000u16,
            prover_instance_poll_time_in_milli_secs: 250u16,
            prometheus_listener_port: 3314,
            prometheus_pushgateway_url: "http://127.0.0.1:9091".to_string(),
            prometheus_push_interval_ms: Some(100),
            prover_group_id: 0,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            CIRCUIT_SYNTHESIZER_GENERATION_TIMEOUT_IN_SECS=1000
            CIRCUIT_SYNTHESIZER_MAX_ATTEMPTS=2
            CIRCUIT_SYNTHESIZER_GPU_PROVER_QUEUE_TIMEOUT_IN_SECS=1000
            CIRCUIT_SYNTHESIZER_PROVER_INSTANCE_WAIT_TIMEOUT_IN_SECS=1000
            CIRCUIT_SYNTHESIZER_PROVER_INSTANCE_POLL_TIME_IN_MILLI_SECS=250
            CIRCUIT_SYNTHESIZER_PROMETHEUS_LISTENER_PORT=3314
            CIRCUIT_SYNTHESIZER_PROMETHEUS_PUSHGATEWAY_URL="http://127.0.0.1:9091"
            CIRCUIT_SYNTHESIZER_PROMETHEUS_PUSH_INTERVAL_MS=100
            CIRCUIT_SYNTHESIZER_PROVER_GROUP_ID=0
        "#;
        lock.set_env(config);

        let actual = CircuitSynthesizerConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
