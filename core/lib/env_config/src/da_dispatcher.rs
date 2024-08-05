use zksync_config::DADispatcherConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for DADispatcherConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("da_dispatcher", "DA_DISPATCHER_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::da_dispatcher::DADispatcherConfig;

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_da_layer_config(
        interval: u32,
        rows_limit: u32,
        max_retries: u16,
    ) -> DADispatcherConfig {
        DADispatcherConfig {
            polling_interval_ms: Some(interval),
            max_rows_to_dispatch: Some(rows_limit),
            max_retries: Some(max_retries),
            enable_onchain_verification: false,
        }
    }

    #[test]
    fn from_env_da_dispatcher() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_DISPATCHER_POLLING_INTERVAL_MS=5000
            DA_DISPATCHER_MAX_ROWS_TO_DISPATCH=60
            DA_DISPATCHER_MAX_RETRIES=7
            DA_DISPATCHER_ENALBE_ONCHAIN_VERIFICATION="false"
        "#;
        lock.set_env(config);
        let actual = DADispatcherConfig::from_env().unwrap();
        assert_eq!(actual, expected_da_layer_config(5000, 60, 7));
    }
}
