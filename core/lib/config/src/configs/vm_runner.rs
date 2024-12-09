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
