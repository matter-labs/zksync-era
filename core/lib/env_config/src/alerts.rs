use crate::{envy_load, FromEnv};
use zksync_config::configs::AlertsConfig;

impl FromEnv for AlertsConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("sporadic_crypto_errors_substrs", "ALERTS_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> AlertsConfig {
        AlertsConfig {
            sporadic_crypto_errors_substrs: vec![
                "EventDestroyErr".to_string(),
                "Can't free memory of DeviceBuf".to_string(),
                "value: PoisonError".to_string(),
            ],
        }
    }

    #[test]
    fn test_from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            ALERTS_SPORADIC_CRYPTO_ERRORS_SUBSTRS="EventDestroyErr,Can't free memory of DeviceBuf,value: PoisonError"
        "#;
        lock.set_env(config);

        assert_eq!(AlertsConfig::from_env().unwrap(), expected_config());
    }
}
