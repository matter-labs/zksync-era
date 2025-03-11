use zksync_config::{configs::secrets::ContractVerifierSecrets, ContractVerifierConfig};

use crate::{envy_load, FromEnv};

impl FromEnv for ContractVerifierConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("contract_verifier", "CONTRACT_VERIFIER_")
    }
}

impl FromEnv for ContractVerifierSecrets {
    fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            etherscan_api_key: std::env::var("CONTRACT_VERIFIER_ETHERSCAN_API_KEY")
                .ok()
                .map(Into::into),
        })
    }
}

#[cfg(test)]
mod tests {
    use zksync_basic_types::secrets::APIKey;

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    #[test]
    fn config_from_env_without_etherscan_api_url() {
        let mut lock = MUTEX.lock();
        let config = r#"
            CONTRACT_VERIFIER_COMPILATION_TIMEOUT=30
            CONTRACT_VERIFIER_PROMETHEUS_PORT=3314
            CONTRACT_VERIFIER_PORT=3070
        "#;
        lock.set_env(config);

        let actual = ContractVerifierConfig::from_env().unwrap();
        assert_eq!(
            actual,
            ContractVerifierConfig {
                compilation_timeout: 30,
                prometheus_port: 3314,
                port: 3070,
                etherscan_api_url: None,
            }
        );
    }

    #[test]
    fn config_from_env_with_etherscan_api_url() {
        let mut lock = MUTEX.lock();
        let config = r#"
            CONTRACT_VERIFIER_COMPILATION_TIMEOUT=30
            CONTRACT_VERIFIER_PROMETHEUS_PORT=3314
            CONTRACT_VERIFIER_PORT=3070
            CONTRACT_VERIFIER_ETHERSCAN_API_URL=http://localhost:8080/api
        "#;
        lock.set_env(config);

        let actual = ContractVerifierConfig::from_env().unwrap();
        assert_eq!(
            actual,
            ContractVerifierConfig {
                compilation_timeout: 30,
                prometheus_port: 3314,
                port: 3070,
                etherscan_api_url: Some("http://localhost:8080/api".to_string()),
            }
        );
    }

    #[test]
    fn secrets_from_env_without_etherscan_api_key() {
        let mut lock = MUTEX.lock();
        let config = r#"
        "#;
        lock.set_env(config);

        let actual = ContractVerifierSecrets::from_env().unwrap();
        assert_eq!(
            actual,
            ContractVerifierSecrets {
                etherscan_api_key: None,
            }
        );
    }

    #[test]
    fn secrets_from_env_with_etherscan_api_key() {
        let mut lock = MUTEX.lock();
        let config = r#"
            CONTRACT_VERIFIER_ETHERSCAN_API_KEY=etherscan-api-key
        "#;
        lock.set_env(config);

        let actual = ContractVerifierSecrets::from_env().unwrap();
        assert_eq!(
            actual,
            ContractVerifierSecrets {
                etherscan_api_key: Some(APIKey::from("etherscan-api-key")),
            }
        );
    }
}
