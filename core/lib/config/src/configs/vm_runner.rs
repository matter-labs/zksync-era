use serde::Deserialize;
use zksync_basic_types::L1BatchNumber;

#[derive(Debug, Deserialize, Clone, PartialEq)]
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

#[derive(Debug, Deserialize, Clone, PartialEq)]
pub struct BasicWitnessInputProducerConfig {
    /// Path to the RocksDB data directory that serves state cache.
    #[serde(default = "BasicWitnessInputProducerConfig::default_db_path")]
    pub db_path: String,
    /// How many max batches should be processed at the same time.
    pub window_size: u32,
    /// All batches before this one (inclusive) are always considered to be processed.
    pub first_processed_batch: L1BatchNumber,
}

impl BasicWitnessInputProducerConfig {
    fn default_db_path() -> String {
        "./db/basic_witness_input_producer".to_owned()
    }
}
