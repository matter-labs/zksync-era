use std::{path::PathBuf, time::Duration};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};
use smart_config::{
    de::{Serde, WellKnown},
    metadata::{SizeUnit, TimeUnit},
    ByteSize, DescribeConfig, DeserializeConfig,
};

use crate::configs::ExperimentalDBConfig;

/// Mode of operation for the Merkle tree.
///
/// The mode does not influence how tree data is stored; i.e., a mode can be switched on the fly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MerkleTreeMode {
    /// In this mode, `MetadataCalculator` will compute commitments and witness inputs for all storage operations
    /// and optionally put witness inputs into the object store as provided by `store_factory` (e.g., GCS).
    #[default]
    Full,
    /// In this mode, `MetadataCalculator` computes Merkle tree root hashes and some auxiliary information
    /// for L1 batches, but not witness inputs.
    Lightweight,
}

impl WellKnown for MerkleTreeMode {
    type Deserializer = Serde![str];
    const DE: Self::Deserializer = Serde![str];
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct MerkleTreeConfig {
    /// Path to the RocksDB data directory for Merkle tree.
    pub path: PathBuf,
    /// Operation mode for the Merkle tree. If not specified, the full mode will be used.
    #[config(default)]
    pub mode: MerkleTreeMode,
    /// Chunk size for multi-get operations. Can speed up loading data for the Merkle tree on some environments,
    /// but the effects vary wildly depending on the setup (e.g., the filesystem used).
    #[config(default_t = 500)]
    pub multi_get_chunk_size: usize,
    /// Capacity of the block cache for the Merkle tree RocksDB. Reasonable values range from ~100 MB to several GB.
    /// The default value is 128 MB.
    #[config(default_t = ByteSize::new(128, SizeUnit::MiB), with = SizeUnit::MiB)]
    pub block_cache_size_mb: ByteSize,
    /// Byte capacity of memtables (recent, non-persisted changes to RocksDB). Setting this to a reasonably
    /// large value (order of 512 MiB) is helpful for large DBs that experience write stalls.
    #[config(default_t = ByteSize::new(256, SizeUnit::MiB), with = SizeUnit::MiB)]
    pub memtable_capacity_mb: ByteSize,
    /// Timeout to wait for the Merkle tree database to run compaction on stalled writes.
    #[config(default_t = Duration::from_secs(30), with = TimeUnit::Seconds)]
    pub stalled_writes_timeout_sec: Duration,
    /// Maximum number of L1 batches to be processed by the Merkle tree at a time.
    #[config(default_t = 20)]
    pub max_l1_batches_per_iter: usize,
}

impl MerkleTreeConfig {
    /// Creates a config for test purposes.
    pub fn for_tests(path: PathBuf) -> Self {
        Self {
            path,
            mode: MerkleTreeMode::default(),
            multi_get_chunk_size: 500,
            block_cache_size_mb: ByteSize::new(128, SizeUnit::MiB),
            memtable_capacity_mb: ByteSize::new(256, SizeUnit::MiB),
            stalled_writes_timeout_sec: Duration::from_secs(30),
            max_l1_batches_per_iter: 20,
        }
    }
}

/// Database configuration.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct DBConfig {
    /// Path to the RocksDB data directory that serves state cache.
    pub state_keeper_db_path: PathBuf,
    /// Merkle tree configuration.
    #[config(nest)]
    pub merkle_tree: MerkleTreeConfig,
    /// Experimental parts of the config.
    #[config(nest)]
    pub experimental: ExperimentalDBConfig,
}

/// Collection of different database URLs and general PostgreSQL options.
/// All the entries are optional, since some components may only require a subset of them,
/// and any component may have overrides.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct PostgresConfig {
    /// Maximum size of the connection pool.
    #[config(alias = "pool_size")]
    pub max_connections: Option<u32>,
    /// Maximum size of the connection pool to master DB.
    #[config(alias = "pool_size_master")]
    pub max_connections_master: Option<u32>,
    /// Acquire timeout in seconds for a single connection attempt. There are multiple attempts (currently 3)
    /// before acquire methods will return an error.
    #[config(default_t = Duration::from_secs(30), with = TimeUnit::Seconds)]
    pub acquire_timeout_sec: Duration,
    /// Statement timeout in seconds for Postgres connections. Applies only to the replica
    /// connection pool used by the API servers.
    #[config(default_t = Duration::from_secs(10), with = TimeUnit::Seconds)]
    pub statement_timeout_sec: Duration,
    /// Threshold in milliseconds for the DB connection lifetime to denote it as long-living and log its details.
    #[config(default_t = Duration::from_secs(5), with = TimeUnit::Millis)]
    pub long_connection_threshold_ms: Duration,
    /// Threshold in milliseconds to denote a DB query as "slow" and log its details.
    #[config(default_t = Duration::from_secs(3), with = TimeUnit::Millis)]
    pub slow_query_threshold_ms: Duration,
}

impl PostgresConfig {
    /// Returns the maximum size of the connection pool as a `Result` to simplify error propagation.
    pub fn max_connections(&self) -> anyhow::Result<u32> {
        self.max_connections.context("Max connections is absent")
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use smart_config::{
        testing::{test_complete, Tester},
        Environment, Yaml,
    };

    use super::*;

    fn assert_db_config(config: &DBConfig) {
        assert_eq!(config.state_keeper_db_path.as_os_str(), "/db/state_keeper");
        assert_eq!(config.merkle_tree.path.as_os_str(), "/db/tree");
        assert_eq!(config.merkle_tree.mode, MerkleTreeMode::Lightweight);
        assert_eq!(config.merkle_tree.multi_get_chunk_size, 250);
        assert_eq!(config.merkle_tree.max_l1_batches_per_iter, 50);
        assert_eq!(config.merkle_tree.memtable_capacity_mb, ByteSize(512 << 20));
        assert_eq!(
            config.merkle_tree.stalled_writes_timeout_sec,
            Duration::from_secs(60)
        );
        assert_eq!(
            config.experimental.state_keeper_db_block_cache_capacity_mb,
            ByteSize(64 << 20)
        );
        assert_eq!(
            config.experimental.state_keeper_db_max_open_files,
            NonZeroU32::new(100)
        );
        assert!(config.experimental.merkle_tree_repair_stale_keys);
        assert!(!config.experimental.protective_reads_persistence_enabled);
    }

    #[test]
    fn db_from_env() {
        let env = r#"
            DATABASE_STATE_KEEPER_DB_PATH="/db/state_keeper"
            DATABASE_MERKLE_TREE_PATH="/db/tree"
            DATABASE_MERKLE_TREE_MODE=lightweight
            DATABASE_MERKLE_TREE_MULTI_GET_CHUNK_SIZE=250
            DATABASE_MERKLE_TREE_MEMTABLE_CAPACITY_MB=512
            DATABASE_MERKLE_TREE_STALLED_WRITES_TIMEOUT_SEC=60
            DATABASE_MERKLE_TREE_MAX_L1_BATCHES_PER_ITER=50
            DATABASE_MERKLE_TREE_BLOCK_CACHE_SIZE_MB=128
            DATABASE_EXPERIMENTAL_STATE_KEEPER_DB_BLOCK_CACHE_CAPACITY_MB=64
            DATABASE_EXPERIMENTAL_PROCESSING_DELAY_MS=0
            DATABASE_EXPERIMENTAL_STATE_KEEPER_DB_MAX_OPEN_FILES=100
            DATABASE_EXPERIMENTAL_MERKLE_TREE_REPAIR_STALE_KEYS=true
            DATABASE_EXPERIMENTAL_PROTECTIVE_READS_PERSISTENCE_ENABLED=false
            DATABASE_EXPERIMENTAL_INCLUDE_INDICES_AND_FILTERS_IN_BLOCK_CACHE=false
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("DATABASE_");

        let config: DBConfig = test_complete(env).unwrap();
        assert_db_config(&config);
    }

    #[test]
    fn db_from_yaml() {
        let yaml = r#"
          state_keeper_db_path: /db/state_keeper
          merkle_tree:
            path: /db/tree
            mode: LIGHTWEIGHT
            multi_get_chunk_size: 250
            block_cache_size_mb: 128
            memtable_capacity_mb: 512
            stalled_writes_timeout_sec: 60
            max_l1_batches_per_iter: 50
          experimental:
            state_keeper_db_block_cache_capacity_mb: 64
            reads_persistence_enabled: false
            processing_delay_ms: 0
            include_indices_and_filters_in_block_cache: false
            merkle_tree_repair_stale_keys: true
            state_keeper_db_max_open_files: 100
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: DBConfig = Tester::default()
            .coerce_variant_names()
            .test_complete(yaml)
            .unwrap();

        assert_db_config(&config);
    }

    fn assert_postgres_config(config: &PostgresConfig) {
        assert_eq!(config.max_connections().unwrap(), 50);
        assert_eq!(config.max_connections_master, Some(20));
        assert_eq!(config.statement_timeout_sec, Duration::from_secs(300));
        assert_eq!(config.acquire_timeout_sec, Duration::from_secs(15));
        assert_eq!(config.long_connection_threshold_ms, Duration::from_secs(3));
        assert_eq!(config.slow_query_threshold_ms, Duration::from_millis(150));
    }

    #[test]
    fn postgres_from_env() {
        let env = r#"
            DATABASE_POOL_SIZE=50
            DATABASE_POOL_SIZE_MASTER=20
            DATABASE_ACQUIRE_TIMEOUT_SEC=15
            DATABASE_STATEMENT_TIMEOUT_SEC=300
            DATABASE_LONG_CONNECTION_THRESHOLD_MS=3000
            DATABASE_SLOW_QUERY_THRESHOLD_MS=150
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("DATABASE_");

        let config: PostgresConfig = test_complete(env).unwrap();
        assert_postgres_config(&config);
    }

    #[test]
    fn postgres_from_yaml() {
        let yaml = r#"
          max_connections: 50
          max_connections_master: 20
          acquire_timeout_sec: 15
          statement_timeout_sec: 300
          long_connection_threshold_ms: 3000
          slow_query_threshold_ms: 150
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: PostgresConfig = test_complete(yaml).unwrap();

        assert_postgres_config(&config);
    }
}
