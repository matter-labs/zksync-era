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
            gpu_prover_queue_reporting_interval_ms: 10_000,
            prover_job_retrying_interval_ms: 10000,
            prover_stats_reporting_interval_ms: 5_000,
            witness_job_moving_interval_ms: 30_000,
            witness_generator_stats_reporting_interval_ms: 10_000,
            witness_generator_job_retrying_interval_ms: 30_000,
            prover_db_pool_size: 2,
            proof_compressor_job_retrying_interval_ms: 30_000,
            proof_compressor_stats_reporting_interval_ms: 10_000,
            prover_job_archiver_archiving_interval_ms: Some(1_800_000),
            prover_job_archiver_archive_after_secs: Some(172_800),
            // 24 hours
            fri_gpu_prover_archiver_archiving_interval_ms: Some(86_400_000),
            // 48 hours
            fri_gpu_prover_archiver_archive_after_secs: Some(172_800),
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            HOUSE_KEEPER_L1_BATCH_METRICS_REPORTING_INTERVAL_MS="10000"
            HOUSE_KEEPER_GPU_PROVER_QUEUE_REPORTING_INTERVAL_MS="10000"
            HOUSE_KEEPER_PROVER_JOB_RETRYING_INTERVAL_MS="10000"
            HOUSE_KEEPER_WITNESS_JOB_MOVING_INTERVAL_MS="30000"
            HOUSE_KEEPER_WITNESS_GENERATOR_STATS_REPORTING_INTERVAL_MS="10000"
            HOUSE_KEEPER_WITNESS_GENERATOR_JOB_RETRYING_INTERVAL_MS="30000"
            HOUSE_KEEPER_FRI_WITNESS_JOB_MOVING_INTERVAL_MS="40000"
            HOUSE_KEEPER_FRI_PROVER_JOB_RETRYING_INTERVAL_MS="30000"
            HOUSE_KEEPER_FRI_WITNESS_GENERATOR_JOB_RETRYING_INTERVAL_MS="30000"
            HOUSE_KEEPER_PROVER_DB_POOL_SIZE="2"
            HOUSE_KEEPER_PROVER_STATS_REPORTING_INTERVAL_MS="5000"
            HOUSE_KEEPER_PROOF_COMPRESSOR_STATS_REPORTING_INTERVAL_MS="10000"
            HOUSE_KEEPER_PROOF_COMPRESSOR_JOB_RETRYING_INTERVAL_MS="30000"
            HOUSE_KEEPER_PROVER_JOB_ARCHIVER_ARCHIVING_INTERVAL_MS="1800000"
            HOUSE_KEEPER_PROVER_JOB_ARCHIVER_ARCHIVE_AFTER_SECS="172800"
            HOUSE_KEEPER_FRI_GPU_PROVER_ARCHIVER_ARCHIVING_INTERVAL_MS="86400000"
            HOUSE_KEEPER_FRI_GPU_PROVER_ARCHIVER_ARCHIVE_AFTER_SECS="172800"
        "#;
        lock.set_env(config);

        let actual = HouseKeeperConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
