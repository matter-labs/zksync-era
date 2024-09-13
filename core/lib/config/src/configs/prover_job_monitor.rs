use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Config used for running ProverJobMonitor.
/// It handles configuration for setup of the binary (like database connections, prometheus) and configuration for jobs that are being ran.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProverJobMonitorConfig {
    /// Port for prometheus metrics connection.
    pub prometheus_port: u16,
    /// Maximum number of database connections per pool.
    /// In a balanced system it should match the number of Tasks ran by ProverJobMonitor.
    /// If lower, components will wait on one another for a connection.
    /// If more, database will use more resources for idle connections (which drains DB resources needed for other components in Prover Subsystems).
    pub max_db_connections: u32,
    /// Amount of time ProverJobMonitor will wait all it's tasks to finish.
    #[serde(default = "ProverJobMonitorConfig::default_graceful_shutdown_timeout_ms")]
    pub graceful_shutdown_timeout_ms: u64,
    /// The interval between runs for GPU Prover Archiver.
    #[serde(default = "ProverJobMonitorConfig::default_gpu_prover_archiver_run_interval_ms")]
    pub gpu_prover_archiver_run_interval_ms: u64,
    /// The amount of time after which 'dead' provers can be archived.
    #[serde(
        default = "ProverJobMonitorConfig::default_gpu_prover_archiver_archive_prover_after_ms"
    )]
    pub gpu_prover_archiver_archive_prover_after_ms: u64,
    /// The interval between runs for Prover Jobs Archiver.
    #[serde(default = "ProverJobMonitorConfig::default_prover_jobs_archiver_run_interval_ms")]
    pub prover_jobs_archiver_run_interval_ms: u64,
    /// The amount of time after which completed jobs (that belong to completed batches) can be archived.
    #[serde(
        default = "ProverJobMonitorConfig::default_prover_jobs_archiver_archive_jobs_after_ms"
    )]
    pub prover_jobs_archiver_archive_jobs_after_ms: u64,
    /// The interval between runs for Proof Compressor Job Requeuer.
    #[serde(
        default = "ProverJobMonitorConfig::default_proof_compressor_job_requeuer_run_interval_ms"
    )]
    pub proof_compressor_job_requeuer_run_interval_ms: u64,
    /// The interval between runs for Prover Job Requeuer.
    #[serde(default = "ProverJobMonitorConfig::default_prover_job_requeuer_run_interval_ms")]
    pub prover_job_requeuer_run_interval_ms: u64,
    /// The interval between runs for Witness Generator Job Requeuer.
    #[serde(
        default = "ProverJobMonitorConfig::default_witness_generator_job_requeuer_run_interval_ms"
    )]
    pub witness_generator_job_requeuer_run_interval_ms: u64,
    /// The interval between runs for Proof Compressor Queue Reporter.
    #[serde(
        default = "ProverJobMonitorConfig::default_proof_compressor_queue_reporter_run_interval_ms"
    )]
    pub proof_compressor_queue_reporter_run_interval_ms: u64,
    /// The interval between runs for Prover Queue Reporter.
    #[serde(default = "ProverJobMonitorConfig::default_prover_queue_reporter_run_interval_ms")]
    pub prover_queue_reporter_run_interval_ms: u64,
    /// The interval between runs for Witness Generator Queue Reporter.
    #[serde(
        default = "ProverJobMonitorConfig::default_witness_generator_queue_reporter_run_interval_ms"
    )]
    pub witness_generator_queue_reporter_run_interval_ms: u64,
    /// The interval between runs for Witness Job Queuer.
    #[serde(default = "ProverJobMonitorConfig::default_witness_job_queuer_run_interval_ms")]
    pub witness_job_queuer_run_interval_ms: u64,
}

impl ProverJobMonitorConfig {
    /// Default graceful shutdown timeout -- 5 seconds
    pub fn default_graceful_shutdown_timeout_ms() -> u64 {
        5_000
    }

    /// Amount of time ProverJobMonitor will wait all it's tasks to finish.
    pub fn graceful_shutdown_timeout(&self) -> Duration {
        Duration::from_millis(self.graceful_shutdown_timeout_ms)
    }

    /// The interval between runs for GPU Prover Archiver.
    pub fn gpu_prover_archiver_run_interval(&self) -> Duration {
        Duration::from_millis(self.gpu_prover_archiver_run_interval_ms)
    }

    /// Default gpu_prover_archiver_archive_prover_after_secs -- 1 day
    pub fn default_gpu_prover_archiver_run_interval_ms() -> u64 {
        86_400_000
    }

    /// The amount of time after which 'dead' provers can be archived.
    pub fn archive_gpu_prover_duration(&self) -> Duration {
        Duration::from_millis(self.gpu_prover_archiver_archive_prover_after_ms)
    }

    /// Default gpu_prover_archiver_archive_prover_after_ms -- 2 days
    pub fn default_gpu_prover_archiver_archive_prover_after_ms() -> u64 {
        172_800_000
    }

    /// The amount of time after which completed jobs (that belong to completed batches) can be archived.
    pub fn prover_jobs_archiver_run_interval(&self) -> Duration {
        Duration::from_millis(self.prover_jobs_archiver_run_interval_ms)
    }

    /// Default prover_jobs_archiver_run_interval_ms -- 30 minutes
    pub fn default_prover_jobs_archiver_run_interval_ms() -> u64 {
        1_800_000
    }
    /// The interval between runs for Prover Jobs Archiver.
    pub fn archive_prover_jobs_duration(&self) -> Duration {
        Duration::from_millis(self.prover_jobs_archiver_archive_jobs_after_ms)
    }

    /// Default prover_jobs_archiver_archive_jobs_after_ms -- 2 days
    pub fn default_prover_jobs_archiver_archive_jobs_after_ms() -> u64 {
        172_800_000
    }

    /// The interval between runs for Proof Compressor Job Requeuer.
    pub fn proof_compressor_job_requeuer_run_interval(&self) -> Duration {
        Duration::from_millis(self.proof_compressor_job_requeuer_run_interval_ms)
    }

    /// Default proof_compressor_job_requeuer_run_interval_ms -- 10 seconds
    pub fn default_proof_compressor_job_requeuer_run_interval_ms() -> u64 {
        10_000
    }

    /// The interval between runs for Prover Job Requeuer.
    pub fn prover_job_requeuer_run_interval(&self) -> Duration {
        Duration::from_millis(self.prover_job_requeuer_run_interval_ms)
    }

    /// Default prover_job_requeuer_run_interval_ms -- 10 seconds
    pub fn default_prover_job_requeuer_run_interval_ms() -> u64 {
        10_000
    }

    /// The interval between runs for Witness Generator Job Requeuer.
    pub fn witness_generator_job_requeuer_run_interval(&self) -> Duration {
        Duration::from_millis(self.witness_generator_job_requeuer_run_interval_ms)
    }

    /// Default witness_generator_job_requeuer_run_interval_ms -- 10 seconds
    pub fn default_witness_generator_job_requeuer_run_interval_ms() -> u64 {
        10_000
    }

    /// The interval between runs for Proof Compressor Queue Reporter.
    pub fn proof_compressor_queue_reporter_run_interval(&self) -> Duration {
        Duration::from_millis(self.proof_compressor_queue_reporter_run_interval_ms)
    }

    /// Default proof_compressor_queue_reporter_run_interval_ms -- 10 seconds
    pub fn default_proof_compressor_queue_reporter_run_interval_ms() -> u64 {
        10_000
    }

    /// The interval between runs for Prover Queue Reporter.
    pub fn prover_queue_reporter_run_interval(&self) -> Duration {
        Duration::from_millis(self.prover_queue_reporter_run_interval_ms)
    }

    /// Default prover_queue_reporter_run_interval_ms -- 10 seconds
    pub fn default_prover_queue_reporter_run_interval_ms() -> u64 {
        10_000
    }

    /// The interval between runs for Witness Generator Queue Reporter.
    pub fn witness_generator_queue_reporter_run_interval(&self) -> Duration {
        Duration::from_millis(self.witness_generator_queue_reporter_run_interval_ms)
    }

    /// Default witness_generator_queue_reporter_run_interval_ms -- 10 seconds
    pub fn default_witness_generator_queue_reporter_run_interval_ms() -> u64 {
        10_000
    }

    /// The interval between runs for Witness Job Queuer.
    pub fn witness_job_queuer_run_interval(&self) -> Duration {
        Duration::from_millis(self.witness_job_queuer_run_interval_ms)
    }

    /// Default witness_job_queuer_run_interval_ms -- 10 seconds
    pub fn default_witness_job_queuer_run_interval_ms() -> u64 {
        10_000
    }
}
