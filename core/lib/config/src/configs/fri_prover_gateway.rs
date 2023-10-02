use super::envy_load;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriProverGatewayConfig {
    pub api_url: String,
    pub api_poll_duration_secs: u16,

    /// Configurations for prometheus
    pub prometheus_listener_port: u16,
    pub prometheus_pushgateway_url: String,
    pub prometheus_push_interval_ms: Option<u64>,
}

impl FriProverGatewayConfig {
    pub fn from_env() -> anyhow::Result<Self> {
        envy_load("fri_prover_gateway", "FRI_PROVER_GATEWAY_")
    }

    pub fn api_poll_duration(&self) -> Duration {
        Duration::from_secs(self.api_poll_duration_secs as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> FriProverGatewayConfig {
        FriProverGatewayConfig {
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
