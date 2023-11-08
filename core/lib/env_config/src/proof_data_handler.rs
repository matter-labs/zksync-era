use zksync_config::configs::ProofDataHandlerConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ProofDataHandlerConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("proof_data_handler", "PROOF_DATA_HANDLER_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::proof_data_handler::ProtocolVersionLoadingMode;

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ProofDataHandlerConfig {
        ProofDataHandlerConfig {
            http_port: 3320,
            proof_generation_timeout_in_secs: 18000,
            protocol_version_loading_mode: ProtocolVersionLoadingMode::FromEnvVar,
            fri_protocol_version_id: 2,
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
            PROOF_DATA_HANDLER_PROOF_GENERATION_TIMEOUT_IN_SECS="18000"
            PROOF_DATA_HANDLER_HTTP_PORT="3320"
            PROOF_DATA_HANDLER_PROTOCOL_VERSION_LOADING_MODE="FromEnvVar"
            PROOF_DATA_HANDLER_FRI_PROTOCOL_VERSION_ID="2"
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = ProofDataHandlerConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
