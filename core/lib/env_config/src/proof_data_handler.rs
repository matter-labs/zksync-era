use zksync_config::configs::ProofDataHandlerConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ProofDataHandlerConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("proof_data_handler", "PROOF_DATA_HANDLER_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ProofDataHandlerConfig {
        ProofDataHandlerConfig {
            http_port: 3320,
            api_url: "2342".to_string(),
            batch_readiness_check_interval_in_secs: 123,
            proof_generation_timeout_in_secs: 18000,
            retry_connection_interval_in_secs: 123,
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
            PROOF_DATA_HANDLER_PROOF_GENERATION_TIMEOUT_IN_SECS="18000"
            PROOF_DATA_HANDLER_HTTP_PORT="3320"
            PROOF_DATA_HANDLER_BATCH_READINESS_CHECK_INTERVAL_IN_SECS="123"
            PROOF_DATA_HANDLER_RETRY_CONNECTION_INTERVAL_IN_SECS="123"
            PROOF_DATA_HANDLER_API_URL="2342"
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = ProofDataHandlerConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
