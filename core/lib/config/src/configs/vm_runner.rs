use std::{num::NonZeroU32, path::PathBuf};

use smart_config::{de::Serde, DescribeConfig, DeserializeConfig};
use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ProtectiveReadsWriterConfig {
    /// Path to the RocksDB data directory that serves state cache.
    #[config(default_t = "./db/protective_reads_writer".into())]
    pub db_path: PathBuf,
    /// How many max batches should be processed at the same time.
    #[config(default_t = NonZeroU32::new(1).unwrap())]
    pub window_size: NonZeroU32,
    /// All batches before this one (inclusive) are always considered to be processed.
    #[config(default, with = Serde![int])]
    pub first_processed_batch: L1BatchNumber,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct BasicWitnessInputProducerConfig {
    /// Path to the RocksDB data directory that serves state cache.
    #[config(default_t = "./db/basic_witness_input_producer".into())]
    pub db_path: PathBuf,
    /// How many max batches should be processed at the same time.
    #[config(default_t = NonZeroU32::new(1).unwrap())]
    pub window_size: NonZeroU32,
    /// All batches before this one (inclusive) are always considered to be processed.
    #[config(default, with = Serde![int])]
    pub first_processed_batch: L1BatchNumber,
}

#[cfg(test)]
mod tests {
    use smart_config::{testing::test_complete, Environment, Yaml};

    use super::*;

    // Alias: `vm_runner.bwip` vs `basic_witness_input_producer`;
    // `vm_runner.protective_reads` vs `protective_reads_writer`
    #[test]
    fn bwip_from_env() {
        let env = r#"
            VM_RUNNER_BWIP_DB_PATH=/db/bwip
            VM_RUNNER_BWIP_WINDOW_SIZE=50
            VM_RUNNER_BWIP_FIRST_PROCESSED_BATCH=123
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("VM_RUNNER_BWIP_");

        let config: BasicWitnessInputProducerConfig = test_complete(env).unwrap();
        assert_eq!(config.db_path.as_os_str(), "/db/bwip");
        assert_eq!(config.window_size, NonZeroU32::new(50).unwrap());
        assert_eq!(config.first_processed_batch, L1BatchNumber(123));
    }

    #[test]
    fn bwip_from_yaml() {
        let yaml = r#"
          db_path: /db/bwip
          window_size: 50
          first_processed_batch: 123
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();

        let config: BasicWitnessInputProducerConfig = test_complete(yaml).unwrap();
        assert_eq!(config.db_path.as_os_str(), "/db/bwip");
        assert_eq!(config.window_size, NonZeroU32::new(50).unwrap());
        assert_eq!(config.first_processed_batch, L1BatchNumber(123));
    }
}
