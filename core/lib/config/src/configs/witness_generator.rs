use std::time::Duration;

// Built-in uses
// External uses
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub enum BasicWitnessGeneratorDataSource {
    FromPostgres,
    FromPostgresShadowBlob,
    FromBlob,
}

/// Configuration for the witness generation
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct WitnessGeneratorConfig {
    /// Max time for witness to be generated
    pub generation_timeout_in_secs: u16,
    /// Currently only a single (largest) key is supported.
    pub initial_setup_key_path: String,
    /// https://storage.googleapis.com/matterlabs-setup-keys-us/setup-keys/setup_2\^26.key
    pub key_download_url: String,
    /// Max attempts for generating witness
    pub max_attempts: u32,
    // Percentage of the blocks that gets proven in the range [0.0, 1.0]
    // when 0.0 implies all blocks are skipped and 1.0 implies all blocks are proven.
    pub blocks_proving_percentage: Option<u8>,
    pub dump_arguments_for_blocks: Vec<u32>,
    // Optional l1 batch number to process block until(inclusive).
    // This parameter is used in case of performing circuit upgrades(VK/Setup keys),
    // to not let witness-generator pick new job and finish all the existing jobs with old circuit.
    pub last_l1_batch_to_process: Option<u32>,
    /// Where will basic Witness Generator load its data from
    pub data_source: BasicWitnessGeneratorDataSource,
}

impl WitnessGeneratorConfig {
    pub fn witness_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_in_secs as u64)
    }

    pub fn last_l1_batch_to_process(&self) -> u32 {
        self.last_l1_batch_to_process.unwrap_or(u32::MAX)
    }
}
