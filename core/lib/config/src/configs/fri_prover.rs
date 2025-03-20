use std::time::Duration;

use serde::Deserialize;

use crate::ObjectStoreConfig;

/// Configuration for the fri prover application
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriProverConfig {
    pub setup_data_path: String,
    pub prometheus_port: u16,
    pub max_attempts: u32,
    pub generation_timeout_in_secs: u16,
    pub prover_object_store: Option<ObjectStoreConfig>,
}

impl FriProverConfig {
    pub fn proof_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_in_secs as u64)
    }
}
