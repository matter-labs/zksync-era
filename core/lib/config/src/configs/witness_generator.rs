use std::time::Duration;

// Built-in uses
// External uses
use serde::Deserialize;

// Local uses
use super::envy_load;

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
}

impl WitnessGeneratorConfig {
    pub fn from_env() -> Self {
        envy_load("witness", "WITNESS_")
    }

    pub fn witness_generation_timeout(&self) -> Duration {
        Duration::from_secs(self.generation_timeout_in_secs as u64)
    }

    pub fn last_l1_batch_to_process(&self) -> u32 {
        self.last_l1_batch_to_process.unwrap_or(u32::MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> WitnessGeneratorConfig {
        WitnessGeneratorConfig {
            generation_timeout_in_secs: 900_u16,
            initial_setup_key_path: "key".to_owned(),
            key_download_url: "value".to_owned(),
            max_attempts: 4,
            blocks_proving_percentage: Some(30),
            dump_arguments_for_blocks: vec![2, 3],
            last_l1_batch_to_process: None,
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            WITNESS_GENERATION_TIMEOUT_IN_SECS=900
            WITNESS_INITIAL_SETUP_KEY_PATH="key"
            WITNESS_KEY_DOWNLOAD_URL="value"
            WITNESS_MAX_ATTEMPTS=4
            WITNESS_DUMP_ARGUMENTS_FOR_BLOCKS="2,3"
            WITNESS_BLOCKS_PROVING_PERCENTAGE="30"
        "#;
        lock.set_env(config);

        let actual = WitnessGeneratorConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
