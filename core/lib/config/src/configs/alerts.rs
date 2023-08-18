// Built-in uses
// External uses
use serde::Deserialize;

use super::envy_load;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct AlertsConfig {
    /// List of panics' messages from external crypto code,
    /// that are sporadic and needed to be handled separately
    pub sporadic_crypto_errors_substrs: Vec<String>,
}

impl AlertsConfig {
    pub fn from_env() -> Self {
        envy_load("sporadic_crypto_errors_substrs", "ALERTS_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> AlertsConfig {
        AlertsConfig {
            sporadic_crypto_errors_substrs: vec![
                "EventDestroyErr".to_string(),
                "Can't free memory of DeviceBuf".to_string(),
                "called `Result::unwrap()` on an `Err` value: PoisonError".to_string(),
            ],
        }
    }

    #[test]
    fn test_from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            ALERTS_SPORADIC_CRYPTO_ERRORS_SUBSTRS=EventDestroyErr,Can't free memory of DeviceBuf,called `Result::unwrap()` on an `Err` value: PoisonError
        "#;
        lock.set_env(config);

        assert_eq!(AlertsConfig::from_env(), expected_config());
    }
}
