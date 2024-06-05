use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
pub struct ProtectiveReadsWriterConfig {
    /// Path to the RocksDB data directory that serves state cache.
    #[serde(default = "ProtectiveReadsWriterConfig::default_protective_reads_db_path")]
    pub protective_reads_db_path: String,
    /// How many max batches should be processed at the same time.
    pub protective_reads_window_size: u32,
}

impl ProtectiveReadsWriterConfig {
    fn default_protective_reads_db_path() -> String {
        "./db/protective_reads_writer".to_owned()
    }
}
