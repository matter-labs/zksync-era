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
            prover_job_retrying_interval_ms: 300_000,
            prover_stats_reporting_interval_ms: 5_000,
            witness_job_moving_interval_ms: 30_000,
            witness_generator_stats_reporting_interval_ms: 10_000,
            fri_witness_job_moving_interval_ms: 40_000,
            fri_prover_job_retrying_interval_ms: 30_000,
            fri_witness_generator_job_retrying_interval_ms: 30_000,
            prover_db_pool_size: 2,
            fri_prover_stats_reporting_interval_ms: 30_000,
            fri_proof_compressor_job_retrying_interval_ms: 30_000,
            fri_proof_compressor_stats_reporting_interval_ms: 30_000,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            HOUSE_KEEPER_L1_BATCH_METRICS_REPORTING_INTERVAL_MS="10000"
            HOUSE_KEEPER_GPU_PROVER_QUEUE_REPORTING_INTERVAL_MS="10000"
            HOUSE_KEEPER_PROVER_JOB_RETRYING_INTERVAL_MS="300000"
            HOUSE_KEEPER_PROVER_STATS_REPORTING_INTERVAL_MS="5000"
            HOUSE_KEEPER_WITNESS_JOB_MOVING_INTERVAL_MS="30000"
            HOUSE_KEEPER_WITNESS_GENERATOR_STATS_REPORTING_INTERVAL_MS="10000"
            HOUSE_KEEPER_FRI_WITNESS_JOB_MOVING_INTERVAL_MS="40000"
            HOUSE_KEEPER_FRI_PROVER_JOB_RETRYING_INTERVAL_MS="30000"
            HOUSE_KEEPER_FRI_WITNESS_GENERATOR_JOB_RETRYING_INTERVAL_MS="30000"
            HOUSE_KEEPER_PROVER_DB_POOL_SIZE="2"
            HOUSE_KEEPER_FRI_PROVER_STATS_REPORTING_INTERVAL_MS="30000"
            HOUSE_KEEPER_FRI_PROOF_COMPRESSOR_STATS_REPORTING_INTERVAL_MS="30000"
            HOUSE_KEEPER_FRI_PROOF_COMPRESSOR_JOB_RETRYING_INTERVAL_MS="30000"
        "#;
        lock.set_env(config);

        let actual = HouseKeeperConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
