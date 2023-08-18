use super::envy_load;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct ProofDataHandlerConfig {
    pub http_port: u16,
    pub proof_generation_timeout_in_secs: u16,
}

impl ProofDataHandlerConfig {
    pub fn from_env() -> Self {
        envy_load("proof_data_handler", "PROOF_DATA_HANDLER_")
    }

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
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
            PROOF_DATA_HANDLER_PROOF_GENERATION_TIMEOUT_IN_SECS="18000"
            PROOF_DATA_HANDLER_HTTP_PORT="3320"
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = ProofDataHandlerConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
