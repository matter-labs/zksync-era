use std::time::Duration;

use serde::{Deserialize, Serialize};
use smart_config::{
    de::{Serde, WellKnown},
    DescribeConfig, DeserializeConfig,
};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct FriProverGatewayConfig {
    pub api_url: String,
    #[config(default_t = Duration::from_secs(1000))]
    pub api_poll_duration: Duration,
    #[config(default)]
    pub api_mode: ApiMode,
    pub port: Option<u16>,
    // Configurations for prometheus
    #[config(default_t = 3314)]
    pub prometheus_listener_port: u16,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub enum ApiMode {
    /// The legacy API mode, which is compatible with the old prover API.
    #[default]
    Legacy,
    /// The new API mode, which is compatible with the prover cluster API.
    ProverCluster,
}

impl WellKnown for ApiMode {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> FriProverGatewayConfig {
        FriProverGatewayConfig {
            api_url: "http://private-dns-for-server".to_string(),
            api_poll_duration: Duration::from_secs(100),
            api_mode: ApiMode::ProverCluster,
            port: Some(8080),
            prometheus_listener_port: 3316,
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            FRI_PROVER_GATEWAY_API_URL="http://private-dns-for-server"
            FRI_PROVER_GATEWAY_API_POLL_DURATION_SECS="100"
            FRI_PROVER_GATEWAY_API_MODE=ProverCluster
            FRI_PROVER_GATEWAY_PORT=8080
            FRI_PROVER_GATEWAY_PROMETHEUS_LISTENER_PORT=3316
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("FRI_PROVER_GATEWAY_");

        let config: FriProverGatewayConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          api_url: http://private-dns-for-server
          api_poll_duration_secs: 100
          api_mode: ProverCluster
          port: 8080
          prometheus_listener_port: 3316
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: FriProverGatewayConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
