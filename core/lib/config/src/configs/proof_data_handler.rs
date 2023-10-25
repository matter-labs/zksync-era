use super::{envy_load, FromEnv};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone, Copy, PartialEq)]
pub enum ProtocolVersionLoadingMode {
    FromDb,
    FromEnvVar,
}

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProofDataHandlerConfig {
    pub http_port: u16,
    pub proof_generation_timeout_in_secs: u16,
    pub protocol_version_loading_mode: ProtocolVersionLoadingMode,
    pub fri_protocol_version_id: u16,
}

impl FromEnv for ProofDataHandlerConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("proof_data_handler", "PROOF_DATA_HANDLER_")
    }
}

impl ProofDataHandlerConfig {
    pub fn proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.proof_generation_timeout_in_secs as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

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
