use zksync_config::configs::CircuitSynthesizerConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for CircuitSynthesizerConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("circuit_synthesizer", "CIRCUIT_SYNTHESIZER_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

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

        let actual = CircuitSynthesizerConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
