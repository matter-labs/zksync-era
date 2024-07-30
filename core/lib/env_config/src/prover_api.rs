use zksync_config::configs::ProverApiConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ProverApiConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("prover_api", "PROVER_API")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ProverApiConfig {
        ProverApiConfig {
            http_port: 3320,
            last_available_batch: 100,
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
            PROVER_API_HTTP_PORT="3320"
            PROVER_API_LAST_AVAILABLE_BATCH=100
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = ProverApiConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
