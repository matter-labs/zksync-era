use std::time::Duration;

// Built-in uses
// External uses
use serde::Deserialize;

// Local uses
use super::envy_load;

/// Configuration for the fri witness generation
#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct FriWitnessGeneratorConfig {
    /// Max time for witness to be generated
    pub generation_timeout_in_secs: u16,
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
    // Force process block with specified number when sampling is enabled.
    pub force_process_block: Option<u32>,
}

impl FriWitnessGeneratorConfig {
    pub fn from_env() -> Self {
        envy_load("fri_witness", "FRI_WITNESS_")
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

    fn expected_config() -> FriWitnessGeneratorConfig {
        FriWitnessGeneratorConfig {
            generation_timeout_in_secs: 900u16,
            max_attempts: 4,
            blocks_proving_percentage: Some(30),
            dump_arguments_for_blocks: vec![2, 3],
            last_l1_batch_to_process: None,
            force_process_block: Some(1),
        }
    }

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        let config = r#"
            FRI_WITNESS_GENERATION_TIMEOUT_IN_SECS=900
            FRI_WITNESS_MAX_ATTEMPTS=4
            FRI_WITNESS_DUMP_ARGUMENTS_FOR_BLOCKS="2,3"
            FRI_WITNESS_BLOCKS_PROVING_PERCENTAGE="30"
            FRI_WITNESS_FORCE_PROCESS_BLOCK="1"
        "#;
        lock.set_env(config);

        let actual = FriWitnessGeneratorConfig::from_env();
        assert_eq!(actual, expected_config());
    }
}
