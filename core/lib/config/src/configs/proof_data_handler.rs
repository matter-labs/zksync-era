use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ProofDataHandlerConfig {
    pub http_port: u16,
    #[config(default_t = 1 * TimeUnit::Minutes)]
    pub proof_generation_timeout: Duration,
    pub gateway_api_url: Option<String>,
    #[config(default_t = Duration::from_secs(10))]
    pub proof_fetch_interval: Duration,
    #[config(default_t = Duration::from_secs(10))]
    pub proof_gen_data_submit_interval: Duration,
    #[config(default_t = true)]
    pub fetch_zero_chain_id_proofs: bool,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> ProofDataHandlerConfig {
        ProofDataHandlerConfig {
            http_port: 3320,
            proof_generation_timeout: Duration::from_secs(18000),
            gateway_api_url: Some("http://gateway/".to_owned()),
            proof_fetch_interval: Duration::from_secs(15),
            proof_gen_data_submit_interval: Duration::from_secs(20),
            fetch_zero_chain_id_proofs: false,
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            PROOF_DATA_HANDLER_PROOF_GENERATION_TIMEOUT_IN_SECS="18000"
            PROOF_DATA_HANDLER_HTTP_PORT="3320"
            PROOF_DATA_HANDLER_GATEWAY_API_URL="http://gateway/"
            PROOF_DATA_HANDLER_PROOF_FETCH_INTERVAL_IN_SECS=15
            PROOF_DATA_HANDLER_PROOF_GEN_DATA_SUBMIT_INTERVAL_IN_SECS=20
            PROOF_DATA_HANDLER_FETCH_ZERO_CHAIN_ID_PROOFS=false
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("PROOF_DATA_HANDLER_");

        let config: ProofDataHandlerConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          http_port: 3320
          proof_generation_timeout_in_secs: 18000
          gateway_api_url: "http://gateway/"
          proof_fetch_interval_in_secs: 15
          proof_gen_data_submit_interval_in_secs: 20
          fetch_zero_chain_id_proofs: false
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ProofDataHandlerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml_with_new_durations() {
        let yaml = r#"
          http_port: 3320
          proof_generation_timeout: 5 hours
          gateway_api_url: "http://gateway/"
          proof_fetch_interval: 15s
          proof_gen_data_submit_interval: 20 secs
          fetch_zero_chain_id_proofs: false
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ProofDataHandlerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
