use zksync_config::ContractVerifierConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ContractVerifierConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("contract_verifier", "CONTRACT_VERIFIER_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ContractVerifierConfig {
        ContractVerifierConfig {
            compilation_timeout: 30,
            polling_interval: Some(1000),
            prometheus_port: 3314,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            CONTRACT_VERIFIER_COMPILATION_TIMEOUT=30
            CONTRACT_VERIFIER_POLLING_INTERVAL=1000
            CONTRACT_VERIFIER_PROMETHEUS_PORT=3314
        "#;
        lock.set_env(config);

        let actual = ContractVerifierConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
