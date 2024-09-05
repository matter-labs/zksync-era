use zksync_config::configs::house_keeper::HouseKeeperConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for HouseKeeperConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("house_keeper", "HOUSE_KEEPER_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> HouseKeeperConfig {
        HouseKeeperConfig {
            l1_batch_metrics_reporting_interval_ms: 10_000,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            HOUSE_KEEPER_L1_BATCH_METRICS_REPORTING_INTERVAL_MS="10000"
        "#;
        lock.set_env(config);

        let actual = HouseKeeperConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
