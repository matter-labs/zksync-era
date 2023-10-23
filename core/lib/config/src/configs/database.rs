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

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MerkleTreeConfig {
    /// Path to the RocksDB data directory for Merkle tree.
    #[serde(default = "MerkleTreeConfig::default_path")]
    pub path: String,
    /// Path to merkle tree backup directory.
    #[serde(default = "MerkleTreeConfig::default_backup_path")]
    pub backup_path: String,
    /// Operation mode for the Merkle tree. If not specified, the full mode will be used.
    #[serde(default)]
    pub mode: MerkleTreeMode,
    /// Chunk size for multi-get operations. Can speed up loading data for the Merkle tree on some environments,
    /// but the effects vary wildly depending on the setup (e.g., the filesystem used).
    #[serde(default = "MerkleTreeConfig::default_multi_get_chunk_size")]
    pub multi_get_chunk_size: usize,
    /// Capacity of the block cache for the Merkle tree RocksDB. Reasonable values range from ~100 MB to several GB.
    /// The default value is 128 MB.
    #[serde(default = "MerkleTreeConfig::default_block_cache_size_mb")]
    pub block_cache_size_mb: usize,
    /// Byte capacity of memtables (recent, non-persisted changes to RocksDB). Setting this to a reasonably
    /// large value (order of 512 MiB) is helpful for large DBs that experience write stalls.
    #[serde(default = "MerkleTreeConfig::default_memtable_capacity_mb")]
    pub memtable_capacity_mb: usize,
    /// Maximum number of L1 batches to be processed by the Merkle tree at a time.
    #[serde(default = "MerkleTreeConfig::default_max_l1_batches_per_iter")]
    pub max_l1_batches_per_iter: usize,
}

impl Default for MerkleTreeConfig {
    fn default() -> Self {
        Self {
            path: Self::default_path(),
            backup_path: Self::default_backup_path(),
            mode: MerkleTreeMode::default(),
            multi_get_chunk_size: Self::default_multi_get_chunk_size(),
            block_cache_size_mb: Self::default_block_cache_size_mb(),
            memtable_capacity_mb: Self::default_memtable_capacity_mb(),
            max_l1_batches_per_iter: Self::default_max_l1_batches_per_iter(),
        }
    }
}

impl MerkleTreeConfig {
    fn default_path() -> String {
        "./db/lightweight-new".to_owned() // named this way for legacy reasons
    }

    fn default_backup_path() -> String {
        "./db/backups".to_owned()
    }

    const fn default_multi_get_chunk_size() -> usize {
        500
    }

    const fn default_block_cache_size_mb() -> usize {
        128
    }

    const fn default_memtable_capacity_mb() -> usize {
        256
    }

    const fn default_max_l1_batches_per_iter() -> usize {
        20
    }

    /// Returns the size of block cache size for Merkle tree in bytes.
    pub fn block_cache_size(&self) -> usize {
        self.block_cache_size_mb * super::BYTES_IN_MEGABYTE
    }

    /// Returns the memtable capacity in bytes.
    pub fn memtable_capacity(&self) -> usize {
        self.memtable_capacity_mb * super::BYTES_IN_MEGABYTE
    }
}

/// Database configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DBConfig {
    /// Statement timeout in seconds for Postgres connections. Applies only to the replica
    /// connection pool used by the API servers.
    pub statement_timeout_sec: Option<u64>,
    /// Path to the RocksDB data directory that serves state cache.
    #[serde(default = "DBConfig::default_state_keeper_db_path")]
    pub state_keeper_db_path: String,
    /// Merkle tree configuration.
    #[serde(skip)]
    // ^ Filled in separately in `Self::from_env()`. We cannot use `serde(flatten)` because it
    // doesn't work with 'envy`.
    pub merkle_tree: MerkleTreeConfig,
    /// Number of backups to keep.
    #[serde(default = "DBConfig::default_backup_count")]
    pub backup_count: usize,
    /// Time interval between performing backups.
    #[serde(default = "DBConfig::default_backup_interval_ms")]
    pub backup_interval_ms: u64,
}

impl DBConfig {
    fn default_state_keeper_db_path() -> String {
        "./db/state_keeper".to_owned()
    }

    const fn default_backup_count() -> usize {
        5
    }

    const fn default_backup_interval_ms() -> u64 {
        60_000
    }

    pub fn from_env() -> anyhow::Result<Self> {
        Ok(Self {
            merkle_tree: envy_load("database_merkle_tree", "DATABASE_MERKLE_TREE_")?,
            ..envy_load("database", "DATABASE_")?
        })
    }

    /// Returns the Postgres statement timeout.
    pub fn statement_timeout(&self) -> Option<Duration> {
        self.statement_timeout_sec.map(Duration::from_secs)
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
        let config = r#"
            DATABASE_STATE_KEEPER_DB_PATH="/db/state_keeper"
            DATABASE_MERKLE_TREE_BACKUP_PATH="/db/backups"
            DATABASE_MERKLE_TREE_PATH="/db/tree"
            DATABASE_MERKLE_TREE_MODE=lightweight
            DATABASE_MERKLE_TREE_MULTI_GET_CHUNK_SIZE=250
            DATABASE_MERKLE_TREE_MAX_L1_BATCHES_PER_ITER=50
            DATABASE_BACKUP_COUNT=5
            DATABASE_BACKUP_INTERVAL_MS=60000
        "#;
        lock.set_env(config);

        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.state_keeper_db_path, "/db/state_keeper");
        assert_eq!(db_config.merkle_tree.path, "/db/tree");
        assert_eq!(db_config.merkle_tree.backup_path, "/db/backups");
        assert_eq!(db_config.merkle_tree.mode, MerkleTreeMode::Lightweight);
        assert_eq!(db_config.merkle_tree.multi_get_chunk_size, 250);
        assert_eq!(db_config.merkle_tree.max_l1_batches_per_iter, 50);
        assert_eq!(db_config.backup_count, 5);
        assert_eq!(db_config.backup_interval().as_secs(), 60);
    }

    #[test]
    fn from_empty_env() {
        let mut lock = MUTEX.lock();
        lock.remove_env(&[
            "DATABASE_STATE_KEEPER_DB_PATH",
            "DATABASE_MERKLE_TREE_BACKUP_PATH",
            "DATABASE_MERKLE_TREE_PATH",
            "DATABASE_MERKLE_TREE_MODE",
            "DATABASE_MERKLE_TREE_MULTI_GET_CHUNK_SIZE",
            "DATABASE_MERKLE_TREE_BLOCK_CACHE_SIZE_MB",
            "DATABASE_MERKLE_TREE_MAX_L1_BATCHES_PER_ITER",
            "DATABASE_BACKUP_COUNT",
            "DATABASE_BACKUP_INTERVAL_MS",
        ]);

        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.state_keeper_db_path, "./db/state_keeper");
        assert_eq!(db_config.merkle_tree.path, "./db/lightweight-new");
        assert_eq!(db_config.merkle_tree.backup_path, "./db/backups");
        assert_eq!(db_config.merkle_tree.mode, MerkleTreeMode::Full);
        assert_eq!(db_config.merkle_tree.multi_get_chunk_size, 500);
        assert_eq!(db_config.merkle_tree.max_l1_batches_per_iter, 20);
        assert_eq!(db_config.merkle_tree.block_cache_size_mb, 128);
        assert_eq!(db_config.backup_count, 5);
        assert_eq!(db_config.backup_interval().as_secs(), 60);

        // Check that new env variable for Merkle tree path is supported
        lock.set_env("DATABASE_MERKLE_TREE_PATH=/db/tree/main");
        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.merkle_tree.path, "/db/tree/main");

        lock.set_env("DATABASE_MERKLE_TREE_MULTI_GET_CHUNK_SIZE=200");
        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.merkle_tree.multi_get_chunk_size, 200);

        lock.set_env("DATABASE_MERKLE_TREE_BLOCK_CACHE_SIZE_MB=256");
        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.merkle_tree.block_cache_size_mb, 256);

        lock.set_env("DATABASE_MERKLE_TREE_MAX_L1_BATCHES_PER_ITER=50");
        let db_config = DBConfig::from_env().unwrap();
        assert_eq!(db_config.merkle_tree.max_l1_batches_per_iter, 50);
    }
}
