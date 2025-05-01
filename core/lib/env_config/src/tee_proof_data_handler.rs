use zksync_config::configs::TeeProofDataHandlerConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for TeeProofDataHandlerConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("proof_data_handler", "TEE_PROOF_DATA_HANDLER_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_basic_types::L1BatchNumber;

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> TeeProofDataHandlerConfig {
        TeeProofDataHandlerConfig {
            http_port: 4320,
            first_processed_batch: L1BatchNumber(1337),
            proof_generation_timeout_in_secs: 600,
            batch_permanently_ignored_timeout_in_hours: 240,
            dcap_collateral_refresh_in_secs: 60,
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
            TEE_PROOF_DATA_HANDLER_HTTP_PORT="4320"
            TEE_PROOF_DATA_HANDLER_FIRST_PROCESSED_BATCH="1337"
            TEE_PROOF_DATA_HANDLER_PROOF_GENERATION_TIMEOUT_IN_SECS="600"
            TEE_PROOF_DATA_HANDLER_BATCH_PERMANENTLY_IGNORED_TIMEOUT_IN_HOURS="240"
            TEE_PROOF_DATA_HANDLER_DCAP_COLLATERAL_REFRESH_IN_SECS=60"
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = TeeProofDataHandlerConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
