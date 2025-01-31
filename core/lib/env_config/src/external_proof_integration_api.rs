use zksync_config::configs::ExternalProofIntegrationApiConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ExternalProofIntegrationApiConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load(
            "external_proof_integration_api",
            "EXTERNAL_PROOF_INTEGRATION_API_",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ExternalProofIntegrationApiConfig {
        ExternalProofIntegrationApiConfig { http_port: 3320 }
    }

    #[test]
    fn from_env() {
        let config = r#"
            EXTERNAL_PROOF_INTEGRATION_API_HTTP_PORT="3320"
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = ExternalProofIntegrationApiConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
