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

    // whether to write to public GCS bucket for https://github.com/matter-labs/era-boojum-validator-cli
    pub shall_save_to_public_bucket: bool,
}

impl FriProverConfig {
    pub fn proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_in_secs as u64)
    }
}
