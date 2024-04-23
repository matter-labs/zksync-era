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
            deny_list: Some("0x1234567890abcdef".to_string()),
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            TX_SINK_DENY_LIST="0x1234567890abcdef"
        "#;
        lock.set_env(config);

        let actual = TxSinkConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
