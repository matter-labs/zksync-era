use zksync_config::configs::ProverJobMonitorConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for ProverJobMonitorConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("prover_job_monitor", "PROVER_JOB_MONITOR_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> ProverJobMonitorConfig {
        ProverJobMonitorConfig {
            prometheus_port: 3317,
            max_db_connections: 9,
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
            PROVER_JOB_MONITOR_PROMETHEUS_PORT=3317
            PROVER_JOB_MONITOR_MAX_DB_CONNECTIONS=9
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = ProverJobMonitorConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
