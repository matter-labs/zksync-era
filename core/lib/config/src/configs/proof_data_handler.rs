use std::time::Duration;

use smart_config::{de::Serde, metadata::TimeUnit, DescribeConfig, DeserializeConfig};
use zksync_basic_types::basic_fri_types::ApiMode;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(validate(
    Self::validate_gateway_api_url,
    "gateway_api_url must be set in `ProverCluster` API mode"
))]
pub struct ProofDataHandlerConfig {
    /// Port to bind proof data handler HTTP REST API to.
    pub http_port: u16,
    /// API mode (`Legacy` or `ProverCluster`).
    // Copy the API mode from the prover gateway config if it's not present locally
    #[config(default, with = Serde![str], alias = "..prover_gateway.api_mode")]
    pub api_mode: ApiMode,
    #[config(default_t = 1 * TimeUnit::Minutes)]
    pub proof_generation_timeout: Duration,
    /// URL of the prover gateway. Must be set in `ProverCluster` API mode.
    pub gateway_api_url: Option<String>,
    #[config(default_t = Duration::from_secs(10))]
    pub proof_fetch_interval: Duration,
    #[config(default_t = Duration::from_secs(10))]
    pub proof_gen_data_submit_interval: Duration,
    #[config(default_t = true)]
    pub fetch_zero_chain_id_proofs: bool,
}

impl ProofDataHandlerConfig {
    fn validate_gateway_api_url(&self) -> bool {
        !matches!(self.api_mode, ApiMode::ProverCluster) || self.gateway_api_url.is_some()
    }
}

#[cfg(test)]
mod tests {
    use smart_config::{
        testing::{test, test_complete, test_minimal, Tester},
        Environment, Yaml,
    };

    use super::*;

    fn expected_config() -> ProofDataHandlerConfig {
        ProofDataHandlerConfig {
            http_port: 3320,
            api_mode: ApiMode::ProverCluster,
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
            PROOF_DATA_HANDLER_API_MODE="ProverCluster"
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
          api_mode: ProverCluster
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
    fn parsing_from_idiomatic_yaml() {
        let yaml = r#"
        data_handler:
          http_port: 3320
          proof_generation_timeout: 5 hours
          gateway_api_url: "http://gateway/"
          proof_fetch_interval: 15s
          proof_gen_data_submit_interval: 20 secs
          fetch_zero_chain_id_proofs: false
        prover_gateway:
          api_mode: ProverCluster
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ProofDataHandlerConfig = Tester::default()
            .insert("data_handler")
            .test_complete(yaml)
            .unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_minimal_yaml() {
        let yaml = r#"
          http_port: 3320
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ProofDataHandlerConfig = test_minimal(yaml).unwrap();
        assert_eq!(config.http_port, 3_320);
    }

    #[test]
    fn validation_error() {
        let yaml = r#"
          http_port: 3320
          api_mode: ProverCluster
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let err = test::<ProofDataHandlerConfig>(yaml).unwrap_err();
        assert_eq!(err.len(), 1);
        let err = err.first();
        assert!(err
            .validation()
            .unwrap()
            .contains("gateway_api_url must be set"));
    }
}
