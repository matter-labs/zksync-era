use zksync_config::configs::TxSinkConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for TxSinkConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("tx_sink", "TX_SINK_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> TxSinkConfig {
        TxSinkConfig {
            deployment_allowlist_sink: true,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            deployment_allowlist_sink=true
        "#;
        lock.set_env(config);

        let actual = TxSinkConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
