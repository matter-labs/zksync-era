use zksync_config::configs::chain::TimestampAsserterConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for TimestampAsserterConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("timestamp_asserter", "TIMESTAMP_ASSERTER_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    #[test]
    fn from_env_timestamp_asserter() {
        let mut lock = MUTEX.lock();
        let config = r#"
            TIMESTAMP_ASSERTER_MIN_RANGE_SEC=1
            TIMESTAMP_ASSERTER_MIN_TIME_TILL_END_SEC=2
        "#;
        lock.set_env(config);

        let actual = TimestampAsserterConfig::from_env().unwrap();
        assert_eq!(
            actual,
            TimestampAsserterConfig {
                min_range_sec: 1,
                min_time_till_end_sec: 2,
            }
        );
    }

    #[test]
    fn from_env_timestamp_asserter_with_defaults() {
        let mut lock = MUTEX.lock();
        lock.remove_env(&[
            "TIMESTAMP_ASSERTER_MIN_RANGE_SEC",
            "TIMESTAMP_ASSERTER_MIN_TIME_TILL_END_SEC",
        ]);

        let actual = TimestampAsserterConfig::from_env().unwrap();
        assert_eq!(
            actual,
            TimestampAsserterConfig {
                min_range_sec: 0,
                min_time_till_end_sec: 0,
            }
        );
    }
}
