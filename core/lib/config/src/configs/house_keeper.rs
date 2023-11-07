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
    pub fri_witness_job_moving_interval_ms: u64,
    pub fri_prover_job_retrying_interval_ms: u64,
    pub fri_witness_generator_job_retrying_interval_ms: u64,
    pub prover_db_pool_size: u32,
    pub fri_prover_stats_reporting_interval_ms: u64,
    pub fri_proof_compressor_job_retrying_interval_ms: u64,
    pub fri_proof_compressor_stats_reporting_interval_ms: u64,
}
