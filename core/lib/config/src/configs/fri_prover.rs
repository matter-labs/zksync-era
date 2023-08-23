use super::envy_load;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub enum SetupLoadMode {
    FromDisk,
    FromMemory,
}

/// Configuration for the fri prover application
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriProverConfig {
    pub setup_data_path: String,
    pub prometheus_port: u16,
    pub max_attempts: u32,
    pub generation_timeout_in_secs: u16,
    pub base_layer_circuit_ids_to_be_verified: Vec<u8>,
    pub recursive_layer_circuit_ids_to_be_verified: Vec<u8>,
    pub setup_load_mode: SetupLoadMode,
    pub specialized_group_id: u8,
    pub witness_vector_generator_thread_count: Option<usize>,
    pub queue_capacity: usize,
    pub witness_vector_receiver_port: u16,
}

impl FriProverConfig {
    pub fn from_env() -> Self {
        envy_load("fri_prover", "FRI_PROVER_")
    }

    pub fn proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_in_secs as u64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

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
        "#;
        lock.set_env(config);

        let actual = FriProverConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
