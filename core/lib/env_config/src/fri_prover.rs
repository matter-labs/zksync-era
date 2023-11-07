use zksync_config::configs::FriProverConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for FriProverConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("fri_prover", "FRI_PROVER_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::fri_prover::SetupLoadMode;

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> FriProverConfig {
        FriProverConfig {
            setup_data_path: "/usr/src/setup-data".to_string(),
            prometheus_port: 3315,
            max_attempts: 10,
            generation_timeout_in_secs: 300,
            base_layer_circuit_ids_to_be_verified: vec![1, 5],
            recursive_layer_circuit_ids_to_be_verified: vec![1, 2, 3],
            setup_load_mode: SetupLoadMode::FromDisk,
            specialized_group_id: 10,
            witness_vector_generator_thread_count: Some(5),
            queue_capacity: 10,
            witness_vector_receiver_port: 3316,
            shall_save_to_public_bucket: true,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            FRI_PROVER_SETUP_DATA_PATH="/usr/src/setup-data"
            FRI_PROVER_PROMETHEUS_PORT="3315"
            FRI_PROVER_MAX_ATTEMPTS="10"
            FRI_PROVER_GENERATION_TIMEOUT_IN_SECS="300"
            FRI_PROVER_BASE_LAYER_CIRCUIT_IDS_TO_BE_VERIFIED="1,5"
            FRI_PROVER_RECURSIVE_LAYER_CIRCUIT_IDS_TO_BE_VERIFIED="1,2,3"
            FRI_PROVER_SETUP_LOAD_MODE="FromDisk"
            FRI_PROVER_SPECIALIZED_GROUP_ID="10"
            FRI_PROVER_WITNESS_VECTOR_GENERATOR_THREAD_COUNT="5"
            FRI_PROVER_QUEUE_CAPACITY="10"
            FRI_PROVER_WITNESS_VECTOR_RECEIVER_PORT="3316"
            FRI_PROVER_SHALL_SAVE_TO_PUBLIC_BUCKET=true
        "#;
        lock.set_env(config);

        let actual = FriProverConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
