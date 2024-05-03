use serde::Deserialize;

/// Configuration for the house keeper.
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct HouseKeeperConfig {
    pub l1_batch_metrics_reporting_interval_ms: u64,
    pub gpu_prover_queue_reporting_interval_ms: u64,
    pub prover_job_retrying_interval_ms: u64,
    pub prover_stats_reporting_interval_ms: u64,
    pub witness_job_moving_interval_ms: u64,
    pub witness_generator_stats_reporting_interval_ms: u64,
    pub witness_generator_job_retrying_interval_ms: u64,
    pub prover_db_pool_size: u32,
    pub proof_compressor_job_retrying_interval_ms: u64,
    pub proof_compressor_stats_reporting_interval_ms: u64,
    pub prover_job_archiver_archiving_interval_ms: Option<u64>,
    pub prover_job_archiver_archive_after_secs: Option<u64>,
    pub fri_gpu_prover_archiver_archiving_interval_ms: Option<u64>,
    pub fri_gpu_prover_archiver_archive_after_secs: Option<u64>,
}

impl HouseKeeperConfig {
    pub fn prover_job_archiver_params(&self) -> Option<(u64, u64)> {
        self.prover_job_archiver_archiving_interval_ms
            .zip(self.prover_job_archiver_archive_after_secs)
    }

    pub fn fri_gpu_prover_archiver_params(&self) -> Option<(u64, u64)> {
        self.fri_gpu_prover_archiver_archiving_interval_ms
            .zip(self.fri_gpu_prover_archiver_archive_after_secs)
    }
}
