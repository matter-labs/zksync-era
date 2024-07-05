use std::time::Duration;

use serde::Deserialize;

use crate::ObjectStoreConfig;

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
    pub setup_load_mode: SetupLoadMode,
    pub specialized_group_id: u8,
    pub queue_capacity: usize,
    pub witness_vector_receiver_port: u16,
    pub zone_read_url: String,
    pub availability_check_interval_in_secs: Option<u32>,

    // whether to write to public GCS bucket for https://github.com/matter-labs/era-boojum-validator-cli
    pub shall_save_to_public_bucket: bool,
    pub prover_object_store: Option<ObjectStoreConfig>,
    pub public_object_store: Option<ObjectStoreConfig>,
}

impl FriProverConfig {
    pub fn proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_in_secs as u64)
    }
}
