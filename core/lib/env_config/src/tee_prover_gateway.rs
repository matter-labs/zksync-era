use zksync_config::configs::TeeProverGatewayConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for TeeProverGatewayConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("tee_prover_gateway", "TEE_PROVER_GATEWAY_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> TeeProverGatewayConfig {
        TeeProverGatewayConfig {
            api_url: "http://private-dns-for-server".to_string(),
            api_poll_duration_secs: 100,
            prometheus_listener_port: 3316,
            prometheus_pushgateway_url: "http://127.0.0.1:9091".to_string(),
            prometheus_push_interval_ms: Some(100),
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
            TEE_PROVER_GATEWAY_API_URL="http://private-dns-for-server"
            TEE_PROVER_GATEWAY_API_POLL_DURATION_SECS="100"
            TEE_PROVER_GATEWAY_PROMETHEUS_LISTENER_PORT=3316
            TEE_PROVER_GATEWAY_PROMETHEUS_PUSHGATEWAY_URL="http://127.0.0.1:9091"
            TEE_PROVER_GATEWAY_PROMETHEUS_PUSH_INTERVAL_MS=100
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = TeeProverGatewayConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
