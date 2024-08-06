use zksync_config::configs::TeeVerifierInputProducerConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for TeeVerifierInputProducerConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("proof_data_handler", "PROOF_DATA_HANDLER_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_basic_types::L2ChainId;

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> TeeVerifierInputProducerConfig {
        TeeVerifierInputProducerConfig {
            l2_chain_id: L2ChainId::from(123),
            max_attempts: 3,
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
            TEE_VERIFIER_INPUT_PRODUCER_L2_CHAIN_ID="123"
            TEE_VERIFIER_INPUT_PRODUCER_MAX_ATTEMPTS="3"
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = TeeVerifierInputProducerConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
