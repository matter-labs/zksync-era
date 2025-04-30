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
            proof_generation_timeout_in_secs: 18000,
            gateway_api_url: None,
            proof_fetch_interval_in_secs:
                ProofDataHandlerConfig::default_proof_fetch_interval_in_secs(),
            proof_gen_data_submit_interval_in_secs:
                ProofDataHandlerConfig::default_proof_gen_data_submit_interval_in_secs(),
            fetch_zero_chain_id_proofs: false,
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
            PROOF_DATA_HANDLER_PROOF_GENERATION_TIMEOUT_IN_SECS="18000"
            PROOF_DATA_HANDLER_HTTP_PORT="3320"
            PROOF_DATA_HANDLER_FETCH_ZERO_CHAIN_ID_PROOFS="false"
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = ProofDataHandlerConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
