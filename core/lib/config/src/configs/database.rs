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
    type Deserializer = Serde![int];
    const DE: Self::Deserializer = Serde![int];
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct MerkleTreeConfig {
    /// Path to the RocksDB data directory for Merkle tree.
    #[config(default_t = "./db/lightweight-new".into())]
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

/// Database configuration.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct DBConfig {
    /// Path to the RocksDB data directory that serves state cache.
    #[config(default_t = "./db/state_keeper".into())]
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
    pub max_connections: Option<u32>,
    /// Maximum size of the connection pool to master DB.
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
