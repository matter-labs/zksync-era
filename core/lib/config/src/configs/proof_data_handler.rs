use std::time::Duration;

use smart_config::{de::Serde, metadata::TimeUnit, DescribeConfig, DeserializeConfig};
use zksync_basic_types::L1BatchNumber;

const SECONDS_IN_DAY: u64 = 86_400;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct TeeConfig {
    /// If true, the TEE support is enabled.
    #[config(default)]
    pub tee_support: bool,
    /// All batches before this one are considered to be processed.
    #[config(default, with = Serde![int])]
    pub first_tee_processed_batch: L1BatchNumber,
    /// Timeout in seconds for retrying the preparation of input for TEE proof generation if it
    /// previously failed (e.g., due to a transient network issue) or if it was picked by a TEE
    /// prover but the TEE proof was not submitted within that time.
    #[config(default_t = Duration::from_secs(60), with = TimeUnit::Seconds)]
    pub tee_proof_generation_timeout_in_secs: Duration,
    /// Timeout in hours after which a batch will be permanently ignored if repeated retries failed.
    #[config(default_t = Duration::from_secs(10 * SECONDS_IN_DAY), with = TimeUnit::Hours)]
    pub tee_batch_permanently_ignored_timeout_in_hours: Duration,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ProofDataHandlerConfig {
    pub http_port: u16,
    #[config(default_t = Duration::from_secs(60), with = TimeUnit::Seconds)]
    pub proof_generation_timeout_in_secs: Duration,
    #[config(flatten)]
    pub tee_config: TeeConfig,
    pub gateway_api_url: Option<String>,
    #[config(default_t = Duration::from_secs(10), with = TimeUnit::Seconds)]
    pub proof_fetch_interval_in_secs: Duration,
    #[config(default_t = Duration::from_secs(10), with = TimeUnit::Seconds)]
    pub proof_gen_data_submit_interval_in_secs: Duration,
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
            proof_generation_timeout_in_secs: Duration::from_secs(18000),
            tee_config: TeeConfig {
                tee_support: true,
                first_tee_processed_batch: L1BatchNumber(1337),
                tee_proof_generation_timeout_in_secs: Duration::from_secs(600),
                tee_batch_permanently_ignored_timeout_in_hours: Duration::from_secs(240 * 3600),
            },
            gateway_api_url: Some("http://gateway/".to_owned()),
            proof_fetch_interval_in_secs: Duration::from_secs(15),
            proof_gen_data_submit_interval_in_secs: Duration::from_secs(20),
            fetch_zero_chain_id_proofs: false,
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            PROOF_DATA_HANDLER_PROOF_GENERATION_TIMEOUT_IN_SECS="18000"
            PROOF_DATA_HANDLER_HTTP_PORT="3320"
            PROOF_DATA_HANDLER_TEE_SUPPORT="true"
            PROOF_DATA_HANDLER_FIRST_TEE_PROCESSED_BATCH="1337"
            PROOF_DATA_HANDLER_TEE_PROOF_GENERATION_TIMEOUT_IN_SECS="600"
            PROOF_DATA_HANDLER_TEE_BATCH_PERMANENTLY_IGNORED_TIMEOUT_IN_HOURS="240"
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
          tee_support: true
          first_tee_processed_batch: 1337
          tee_proof_generation_timeout_in_secs: 600
          tee_batch_permanently_ignored_timeout_in_hours: 240
          gateway_api_url: "http://gateway/"
          proof_fetch_interval_in_secs: 15
          proof_gen_data_submit_interval_in_secs: 20
          fetch_zero_chain_id_proofs: false
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ProofDataHandlerConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
