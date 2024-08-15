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
            graceful_shutdown_timeout_ms: 5000,
            gpu_prover_archiver_run_interval_ms: 86400000,
            gpu_prover_archiver_archive_prover_after_secs: 172800,
            prover_jobs_archiver_run_interval_ms: 1800000,
            prover_jobs_archiver_archive_jobs_after_secs: 172800,
            proof_compressor_job_requeuer_run_interval_ms: 10000,
            prover_job_requeuer_run_interval_ms: 10000,
            witness_generator_job_requeuer_run_interval_ms: 10000,
            proof_compressor_queue_reporter_run_interval_ms: 10000,
            prover_queue_reporter_run_interval_ms: 10000,
            witness_generator_queue_reporter_run_interval_ms: 10000,
            witness_job_queuer_run_interval_ms: 10000,
        }
    }

    #[test]
    fn from_env() {
        let config = r#"
            PROVER_JOB_MONITOR_PROMETHEUS_PORT=3317
            PROVER_JOB_MONITOR_MAX_DB_CONNECTIONS=9
            PROVER_JOB_MONITOR_GRACEFUL_SHUTDOWN_TIMEOUT_MS=5000
            PROVER_JOB_MONITOR_GPU_PROVER_ARCHIVER_RUN_INTERVAL_MS=86400000
            PROVER_JOB_MONITOR_GPU_PROVER_ARCHIVER_ARCHIVE_PROVER_AFTER_SECS=172800
            PROVER_JOB_MONITOR_PROVER_JOBS_ARCHIVER_RUN_INTERVAL_MS=1800000
            PROVER_JOB_MONITOR_PROVER_JOBS_ARCHIVER_ARCHIVE_JOBS_AFTER_SECS=172800
            PROVER_JOB_MONITOR_PROOF_COMPRESSOR_JOB_REQUEUER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_PROVER_JOB_REQUEUER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_WITNESS_GENERATOR_JOB_REQUEUER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_PROOF_COMPRESSOR_QUEUE_REPORTER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_PROVER_QUEUE_REPORTER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_WITNESS_GENERATOR_QUEUE_REPORTER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_WITNESS_JOB_QUEUER_RUN_INTERVAL_MS=10000
        "#;
        let mut lock = MUTEX.lock();
        lock.set_env(config);
        let actual = ProverJobMonitorConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
