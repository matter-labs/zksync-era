use std::time::Duration;

use smart_config::{DescribeConfig, DeserializeConfig};

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
    #[config(default_t = Duration::from_secs(86_400))]
    pub gpu_prover_archiver_run_interval: Duration,
    /// The amount of time after which 'dead' provers can be archived.
    #[config(default_t = Duration::from_secs(86_400 * 2))]
    pub gpu_prover_archiver_archive_prover_after: Duration,
    /// The interval between runs for Prover Jobs Archiver.
    #[config(default_t = Duration::from_secs(1_800))]
    pub prover_jobs_archiver_run_interval: Duration,
    /// The amount of time after which completed jobs (that belong to completed batches) can be archived.
    #[config(default_t = Duration::from_secs(86_400 * 2))]
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
