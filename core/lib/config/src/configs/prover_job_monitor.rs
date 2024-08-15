use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct ProverJobMonitorConfig {
    pub prometheus_port: u16,
    pub max_db_connections: u32,
    pub graceful_shutdown_timeout_ms: u32,
    pub gpu_prover_archiver_run_interval_ms: u64,
    pub gpu_prover_archiver_archive_prover_after_secs: u64,
    pub prover_jobs_archiver_run_interval_ms: u64,
    pub prover_jobs_archiver_archive_jobs_after_secs: u64,
    pub proof_compressor_job_requeuer_run_interval_ms: u64,
    pub prover_job_requeuer_run_interval_ms: u64,
    pub witness_generator_job_requeuer_run_interval_ms: u64,
    pub proof_compressor_queue_reporter_run_interval_ms: u64,
    pub prover_queue_reporter_run_interval_ms: u64,
    pub witness_generator_queue_reporter_run_interval_ms: u64,
    pub witness_job_queuer_run_interval_ms: u64,
}
