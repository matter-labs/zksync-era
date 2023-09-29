use serde::{Deserialize, Serialize};

use std::time::Duration;

use super::envy_load;

/// Mode of operation for the Merkle tree.
///
/// The mode does not influence how tree data is stored; i.e., a mode can be switched on the fly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MerkleTreeMode {
    /// In this mode, `MetadataCalculator` will compute witness inputs for all storage operations
    /// and put them into the object store as provided by `store_factory` (e.g., GCS).
    #[default]
    Full,
    /// In this mode, `MetadataCalculator` computes Merkle tree root hashes and some auxiliary information
    /// for L1 batches, but not witness inputs.
    Lightweight,
}

/// Database configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DBConfig {
    /// Path to the RocksDB data directory that serves state cache.
    #[serde(default = "DBConfig::default_state_keeper_db_path")]
    pub state_keeper_db_path: String,
    /// Path to merkle tree backup directory.
    #[serde(default = "DBConfig::default_merkle_tree_backup_path")]
    pub merkle_tree_backup_path: String,
    /// Path to the RocksDB data directory for Merkle tree.
    #[serde(
        alias = "new_merkle_tree_ssd_path",
        default = "DBConfig::default_merkle_tree_path"
    )]
    pub merkle_tree_path: String,
    /// Operation mode for the Merkle tree. If not specified, the full mode will be used.
    #[serde(default)]
    pub merkle_tree_mode: MerkleTreeMode,
    /// Capacity of the block cache for the Merkle tree RocksDB. Reasonable values range from ~100 MB to several GB.
    /// The default value is 128 MB.
    #[serde(default = "DBConfig::default_merkle_tree_block_cache_size_mb")]
    pub merkle_tree_block_cache_size_mb: usize,
    /// Number of backups to keep.
    #[serde(default = "DBConfig::default_backup_count")]
    pub backup_count: usize,
    /// Time interval between performing backups.
    #[serde(default = "DBConfig::default_backup_interval_ms")]
    pub backup_interval_ms: u64,
    /// Maximum number of L1 batches to be processed by the Merkle tree at a time.
    #[serde(
        alias = "max_block_batch",
        default = "DBConfig::default_max_l1_batches_per_tree_iter"
    )]
    pub max_l1_batches_per_tree_iter: usize,
}

impl DBConfig {
    fn default_state_keeper_db_path() -> String {
        "./db/state_keeper".to_owned()
    }

    fn default_merkle_tree_backup_path() -> String {
        "./db/backups".to_owned()
    }

    fn default_merkle_tree_path() -> String {
        "./db/lightweight-new".to_owned() // named this way for legacy reasons
    }

    const fn default_merkle_tree_block_cache_size_mb() -> usize {
        128
    }

    const fn default_backup_count() -> usize {
        5
    }

    const fn default_backup_interval_ms() -> u64 {
        60_000
    }

    const fn default_max_l1_batches_per_tree_iter() -> usize {
        20
    }

    pub fn from_env() -> Self {
        envy_load("database", "DATABASE_")
    }

    /// Returns the size of block cache size for Merkle tree in bytes.
    pub fn merkle_tree_block_cache_size(&self) -> usize {
        self.merkle_tree_block_cache_size_mb * super::BYTES_IN_MEGABYTE
    }

    pub fn backup_interval(&self) -> Duration {
        Duration::from_millis(self.backup_interval_ms)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::configs::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    #[test]
    fn from_env() {
        let mut lock = MUTEX.lock();
        // Use the legacy naming for some vars to ensure that it works
        lock.remove_env(&[
            "DATABASE_MERKLE_TREE_PATH",
            "DATABASE_MAX_L1_BATCHES_PER_ITER",
        ]);
        let config = r#"
            DATABASE_STATE_KEEPER_DB_PATH="/db/state_keeper"
            DATABASE_MERKLE_TREE_BACKUP_PATH="/db/backups"
            DATABASE_NEW_MERKLE_TREE_SSD_PATH="/db/tree"
            DATABASE_MERKLE_TREE_MODE=lightweight
            DATABASE_BACKUP_COUNT=5
            DATABASE_BACKUP_INTERVAL_MS=60000
            DATABASE_MAX_BLOCK_BATCH=50
        "#;
        lock.set_env(config);

        let db_config = DBConfig::from_env();
        assert_eq!(db_config.state_keeper_db_path, "/db/state_keeper");
        assert_eq!(db_config.merkle_tree_path, "/db/tree");
        assert_eq!(db_config.merkle_tree_backup_path, "/db/backups");
        assert_eq!(db_config.merkle_tree_mode, MerkleTreeMode::Lightweight);
        assert_eq!(db_config.backup_count, 5);
        assert_eq!(db_config.backup_interval().as_secs(), 60);
        assert_eq!(db_config.max_l1_batches_per_tree_iter, 50);
    }

    #[test]
    fn from_empty_env() {
        let mut lock = MUTEX.lock();
        lock.remove_env(&[
            "DATABASE_STATE_KEEPER_DB_PATH",
            "DATABASE_MERKLE_TREE_BACKUP_PATH",
            "DATABASE_MERKLE_TREE_PATH",
            "DATABASE_NEW_MERKLE_TREE_SSD_PATH",
            "DATABASE_MERKLE_TREE_MODE",
            "DATABASE_MERKLE_TREE_BLOCK_CACHE_SIZE_MB",
            "DATABASE_BACKUP_COUNT",
            "DATABASE_BACKUP_INTERVAL_MS",
            "DATABASE_MAX_BLOCK_BATCH",
            "DATABASE_MAX_L1_BATCHES_PER_TREE_ITER",
        ]);

        let db_config = DBConfig::from_env();
        assert_eq!(db_config.state_keeper_db_path, "./db/state_keeper");
        assert_eq!(db_config.merkle_tree_path, "./db/lightweight-new");
        assert_eq!(db_config.merkle_tree_backup_path, "./db/backups");
        assert_eq!(db_config.merkle_tree_mode, MerkleTreeMode::Full);
        assert_eq!(db_config.backup_count, 5);
        assert_eq!(db_config.backup_interval().as_secs(), 60);
        assert_eq!(db_config.max_l1_batches_per_tree_iter, 20);
        assert_eq!(db_config.merkle_tree_block_cache_size_mb, 128);

        // Check that new env variable for Merkle tree path is supported
        lock.set_env("DATABASE_MERKLE_TREE_PATH=/db/tree/main");
        let db_config = DBConfig::from_env();
        assert_eq!(db_config.merkle_tree_path, "/db/tree/main");

        lock.set_env("DATABASE_MERKLE_TREE_BLOCK_CACHE_SIZE_MB=256");
        let db_config = DBConfig::from_env();
        assert_eq!(db_config.merkle_tree_block_cache_size_mb, 256);

        lock.set_env("DATABASE_MAX_L1_BATCHES_PER_TREE_ITER=50");
        let db_config = DBConfig::from_env();
        assert_eq!(db_config.max_l1_batches_per_tree_iter, 50);
    }
}
