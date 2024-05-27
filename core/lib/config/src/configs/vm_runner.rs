use serde::Deserialize;

#[derive(Debug, Deserialize, Clone, PartialEq, Default)]
pub struct VmRunnerConfig {
    /// Path to the RocksDB data directory that serves state cache.
    #[serde(default = "VmRunnerConfig::default_protective_reads_db_path")]
    pub protective_reads_db_path: String,
    /// How many max batches should be processed at the same time.
    pub protective_reads_window_size: u32,
}

impl VmRunnerConfig {
    fn default_protective_reads_db_path() -> String {
        "./db/protective_reads".to_owned()
    }
}
