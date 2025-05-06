use zksync_config::configs::FriProverGatewayConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for FriProverGatewayConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("fri_prover_gateway", "FRI_PROVER_GATEWAY_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::fri_prover_gateway::ApiMode;

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> FriProverGatewayConfig {
        FriProverGatewayConfig {
            api_url: "http://private-dns-for-server".to_string(),
            api_poll_duration_secs: 100,
            prometheus_listener_port: 3316,
            prometheus_pushgateway_url: "http://127.0.0.1:9091".to_string(),
            prometheus_push_interval_ms: Some(100),
            api_mode: ApiMode::Legacy,
            port: None,
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
            FRI_PROVER_GATEWAY_API_URL="http://private-dns-for-server"
            FRI_PROVER_GATEWAY_API_POLL_DURATION_SECS="100"
            FRI_PROVER_GATEWAY_PROMETHEUS_LISTENER_PORT=3316
            FRI_PROVER_GATEWAY_PROMETHEUS_PUSHGATEWAY_URL="http://127.0.0.1:9091"
            FRI_PROVER_GATEWAY_PROMETHEUS_PUSH_INTERVAL_MS=100
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = FriProverGatewayConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
