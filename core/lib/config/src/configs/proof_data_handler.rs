use std::time::Duration;

use serde::{Deserialize, Serialize};
use smart_config::{de::WellKnown, metadata::TimeUnit, DescribeConfig, DeserializeConfig, Serde};

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
    #[config(default_t = ProvingMode::ProverCluster)]
    pub proving_mode: ProvingMode,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProvingMode {
    ProvingNetwork,
    ProverCluster,
}

impl ProvingMode {
    pub fn status_for_dal(&self) -> String {
        match self {
            ProvingMode::ProvingNetwork => "fallbacked".to_string(),
            ProvingMode::ProverCluster => "unpicked".to_string(),
        }
    }
}

impl WellKnown for ProvingMode {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
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
            proving_mode: ProvingMode::ProverCluster,
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
            PROOF_DATA_HANDLER_PROVING_MODE=PROVER_CLUSTER
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
          proving_mode: PROVER_CLUSTER
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ProofDataHandlerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_idiomatic_yaml() {
        let yaml = r#"
          http_port: 3320
          proof_generation_timeout: 5 hours
          gateway_api_url: "http://gateway/"
          proof_fetch_interval: 15s
          proof_gen_data_submit_interval: 20 secs
          fetch_zero_chain_id_proofs: false
          proving_mode: PROVER_CLUSTER
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ProofDataHandlerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
