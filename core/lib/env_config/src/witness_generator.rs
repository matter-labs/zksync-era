use zksync_config::configs::WitnessGeneratorConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for WitnessGeneratorConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("witness", "WITNESS_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::witness_generator::BasicWitnessGeneratorDataSource;

    use super::*;
    use crate::test_utils::EnvMutex;

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
            data_source: BasicWitnessGeneratorDataSource::FromBlob,
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
            WITNESS_DATA_SOURCE="FromBlob"
        "#;
        lock.set_env(config);

        let actual = WitnessGeneratorConfig::from_env().unwrap();
        assert_eq!(actual, expected_config());
    }
}
