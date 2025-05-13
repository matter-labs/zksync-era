use std::time::Duration;

use smart_config::{metadata::TimeUnit, DescribeConfig, DeserializeConfig};

/// Config used for running ProverJobMonitor.
/// It handles configuration for setup of the binary (like database connections, prometheus) and configuration for jobs that are being ran.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct ProverJobMonitorConfig {
    /// Port for prometheus metrics connection.
    pub prometheus_port: u16,
    /// Maximum number of database connections per pool.
    /// In a balanced system it should match the number of Tasks ran by ProverJobMonitor.
    /// If lower, components will wait on one another for a connection.
    /// If more, database will use more resources for idle connections (which drains DB resources needed for other components in Prover Subsystems).
    #[config(default_t = 10)]
    pub max_db_connections: u32,
    /// Amount of time ProverJobMonitor will wait all it's tasks to finish.
    #[config(default_t = Duration::from_secs(5))]
    pub graceful_shutdown_timeout: Duration,
    /// The interval between runs for GPU Prover Archiver.
    #[config(default_t = 1 * TimeUnit::Days)]
    pub gpu_prover_archiver_run_interval: Duration,
    /// The amount of time after which 'dead' provers can be archived.
    #[config(default_t = 2 * TimeUnit::Days)]
    pub gpu_prover_archiver_archive_prover_after: Duration,
    /// The interval between runs for Prover Jobs Archiver.
    #[config(default_t = 30 * TimeUnit::Minutes)]
    pub prover_jobs_archiver_run_interval: Duration,
    /// The amount of time after which completed jobs (that belong to completed batches) can be archived.
    #[config(default_t = 2 * TimeUnit::Days)]
    pub prover_jobs_archiver_archive_jobs_after: Duration,
    /// The interval between runs for Proof Compressor Job Requeuer.
    #[config(default_t = Duration::from_secs(10))]
    pub proof_compressor_job_requeuer_run_interval: Duration,
    /// The interval between runs for Prover Job Requeuer.
    #[config(default_t = Duration::from_secs(10))]
    pub prover_job_requeuer_run_interval: Duration,
    /// The interval between runs for Witness Generator Job Requeuer.
    #[config(default_t = Duration::from_secs(10))]
    pub witness_generator_job_requeuer_run_interval: Duration,
    /// The interval between runs for Proof Compressor Queue Reporter.
    #[config(default_t = Duration::from_secs(10))]
    pub proof_compressor_queue_reporter_run_interval: Duration,
    /// The interval between runs for Prover Queue Reporter.
    #[config(default_t = Duration::from_secs(10))]
    pub prover_queue_reporter_run_interval: Duration,
    /// The interval between runs for Witness Generator Queue Reporter.
    #[config(default_t = Duration::from_secs(10))]
    pub witness_generator_queue_reporter_run_interval: Duration,
    /// The interval between runs for Witness Job Queuer.
    #[config(default_t = Duration::from_secs(10))]
    pub witness_job_queuer_run_interval: Duration,
    /// HTTP port of the ProverJobMonitor to send requests to.
    pub http_port: u16,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    fn expected_config() -> ProverJobMonitorConfig {
        ProverJobMonitorConfig {
            prometheus_port: 3317,
            max_db_connections: 9,
            graceful_shutdown_timeout: Duration::from_secs(5),
            gpu_prover_archiver_run_interval: Duration::from_secs(86400),
            gpu_prover_archiver_archive_prover_after: Duration::from_secs(172800),
            prover_jobs_archiver_run_interval: Duration::from_secs(1800),
            prover_jobs_archiver_archive_jobs_after: Duration::from_secs(172800),
            proof_compressor_job_requeuer_run_interval: Duration::from_secs(10),
            prover_job_requeuer_run_interval: Duration::from_secs(10),
            witness_generator_job_requeuer_run_interval: Duration::from_secs(10),
            proof_compressor_queue_reporter_run_interval: Duration::from_secs(10),
            prover_queue_reporter_run_interval: Duration::from_secs(10),
            witness_generator_queue_reporter_run_interval: Duration::from_secs(10),
            witness_job_queuer_run_interval: Duration::from_secs(10),
            http_port: 3074,
        }
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            PROVER_JOB_MONITOR_PROMETHEUS_PORT=3317
            PROVER_JOB_MONITOR_MAX_DB_CONNECTIONS=9
            PROVER_JOB_MONITOR_GRACEFUL_SHUTDOWN_TIMEOUT_MS=5000
            PROVER_JOB_MONITOR_GPU_PROVER_ARCHIVER_RUN_INTERVAL_MS=86400000
            PROVER_JOB_MONITOR_GPU_PROVER_ARCHIVER_ARCHIVE_PROVER_AFTER_MS=172800000
            PROVER_JOB_MONITOR_PROVER_JOBS_ARCHIVER_RUN_INTERVAL_MS=1800000
            PROVER_JOB_MONITOR_PROVER_JOBS_ARCHIVER_ARCHIVE_JOBS_AFTER_MS=172800000
            PROVER_JOB_MONITOR_PROOF_COMPRESSOR_JOB_REQUEUER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_PROVER_JOB_REQUEUER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_WITNESS_GENERATOR_JOB_REQUEUER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_PROOF_COMPRESSOR_QUEUE_REPORTER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_PROVER_QUEUE_REPORTER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_WITNESS_GENERATOR_QUEUE_REPORTER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_WITNESS_JOB_QUEUER_RUN_INTERVAL_MS=10000
            PROVER_JOB_MONITOR_HTTP_PORT=3074
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("PROVER_JOB_MONITOR_");

        let config: ProverJobMonitorConfig = test_complete(env).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
          prometheus_port: 3317
          max_db_connections: 9
          graceful_shutdown_timeout_ms: 5000
          gpu_prover_archiver_run_interval_ms: 86400000
          gpu_prover_archiver_archive_prover_after_ms: 172800000
          prover_jobs_archiver_run_interval_ms: 1800000
          prover_jobs_archiver_archive_jobs_after_ms: 172800000
          proof_compressor_job_requeuer_run_interval_ms: 10000
          prover_job_requeuer_run_interval_ms: 10000
          witness_generator_job_requeuer_run_interval_ms: 10000
          proof_compressor_queue_reporter_run_interval_ms: 10000
          prover_queue_reporter_run_interval_ms: 10000
          witness_generator_queue_reporter_run_interval_ms: 10000
          witness_job_queuer_run_interval_ms: 10000
          http_port: 3074
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ProverJobMonitorConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_idiomatic_durations_from_yaml() {
        let yaml = r#"
          prometheus_port: 3317
          max_db_connections: 9
          graceful_shutdown_timeout: '5s'
          gpu_prover_archiver_run_interval: '1 day'
          gpu_prover_archiver_archive_prover_after: '2 days'
          prover_jobs_archiver_run_interval: '30 min'
          prover_jobs_archiver_archive_jobs_after: '2 days'
          proof_compressor_job_requeuer_run_interval: '10s'
          prover_job_requeuer_run_interval: '10s'
          witness_generator_job_requeuer_run_interval: '10 seconds'
          proof_compressor_queue_reporter_run_interval: '10s'
          prover_queue_reporter_run_interval: '10 sec'
          witness_generator_queue_reporter_run_interval: '10s'
          witness_job_queuer_run_interval: '10s'
          http_port: 3074
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ProverJobMonitorConfig = test_complete(yaml).unwrap();
        assert_eq!(config, expected_config());
    }
}
