use zksync_config::configs::FriWitnessGeneratorConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for FriWitnessGeneratorConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("fri_witness", "FRI_WITNESS_")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_config() -> FriWitnessGeneratorConfig {
        FriWitnessGeneratorConfig {
            generation_timeout_in_secs: 900u16,
            max_attempts: 4,
            blocks_proving_percentage: Some(30),
            dump_arguments_for_blocks: vec![2, 3],
            last_l1_batch_to_process: None,
            force_process_block: Some(1),
            shall_save_to_public_bucket: true,
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
            FRI_WITNESS_SHALL_SAVE_TO_PUBLIC_BUCKET=true
        "#;
        lock.set_env(config);

        let actual = FriWitnessGeneratorConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
