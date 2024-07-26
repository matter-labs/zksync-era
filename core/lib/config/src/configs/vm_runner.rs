use serde::Deserialize;
pub use zksync_basic_types::vm::FastVmMode;
use zksync_basic_types::L1BatchNumber;

use crate::configs::experimental::ExperimentalVmConfig;

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
pub struct ProtectiveReadsWriterConfig {
    /// Path to the RocksDB data directory that serves state cache.
    #[serde(default = "ProtectiveReadsWriterConfig::default_db_path")]
    pub db_path: String,
    /// How many max batches should be processed at the same time.
    pub window_size: u32,
    /// All batches before this one (inclusive) are always considered to be processed.
    pub first_processed_batch: L1BatchNumber,
}

impl ProtectiveReadsWriterConfig {
    fn default_db_path() -> String {
        "./db/protective_reads_writer".to_owned()
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
pub struct BasicWitnessInputProducerConfig {
    /// Path to the RocksDB data directory that serves state cache.
    #[serde(default = "BasicWitnessInputProducerConfig::default_db_path")]
    pub db_path: String,
    /// How many max batches should be processed at the same time.
    pub window_size: u32,
    /// All batches before this one (inclusive) are always considered to be processed.
    pub first_processed_batch: L1BatchNumber,

    #[serde(skip)]
    // manually added when parsing from env because `envy` doesn't support `serde(flatten)`
    pub experimental_vm: ExperimentalVmConfig,
}

impl BasicWitnessInputProducerConfig {
    fn default_db_path() -> String {
        "./db/basic_witness_input_producer".to_owned()
    }
}
