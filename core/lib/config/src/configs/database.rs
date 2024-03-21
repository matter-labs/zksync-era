use std::time::Duration;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct MerkleTreeConfig {
    /// Path to the RocksDB data directory for Merkle tree.
    #[serde(default = "MerkleTreeConfig::default_path")]
    pub path: String,
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
    /// Timeout to wait for the Merkle tree database to run compaction on stalled writes.
    #[serde(default = "MerkleTreeConfig::default_stalled_writes_timeout_sec")]
    pub stalled_writes_timeout_sec: u64,
    /// Maximum number of L1 batches to be processed by the Merkle tree at a time.
    #[serde(default = "MerkleTreeConfig::default_max_l1_batches_per_iter")]
    pub max_l1_batches_per_iter: usize,
}

impl Default for MerkleTreeConfig {
    fn default() -> Self {
        Self {
            path: Self::default_path(),
            mode: MerkleTreeMode::default(),
            multi_get_chunk_size: Self::default_multi_get_chunk_size(),
            block_cache_size_mb: Self::default_block_cache_size_mb(),
            memtable_capacity_mb: Self::default_memtable_capacity_mb(),
            stalled_writes_timeout_sec: Self::default_stalled_writes_timeout_sec(),
            max_l1_batches_per_iter: Self::default_max_l1_batches_per_iter(),
        }
    }
}

impl MerkleTreeConfig {
    fn default_path() -> String {
        "./db/lightweight-new".to_owned() // named this way for legacy reasons
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

    const fn default_stalled_writes_timeout_sec() -> u64 {
        30
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

    /// Returns the timeout to wait for the Merkle tree database to run compaction on stalled writes.
    pub fn stalled_writes_timeout(&self) -> Duration {
        Duration::from_secs(self.stalled_writes_timeout_sec)
    }
}

/// Database configuration.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct DBConfig {
    /// Path to the RocksDB data directory that serves state cache.
    #[serde(default = "DBConfig::default_state_keeper_db_path")]
    pub state_keeper_db_path: String,
    /// Merkle tree configuration.
    #[serde(skip)]
    // ^ Filled in separately in `Self::from_env()`. We cannot use `serde(flatten)` because it
    // doesn't work with 'envy`.
    pub merkle_tree: MerkleTreeConfig,
}

impl DBConfig {
    fn default_state_keeper_db_path() -> String {
        "./db/state_keeper".to_owned()
    }
}

/// Collection of different database URLs and general PostgreSQL options.
/// All the entries are optional, since some components may only require a subset of them,
/// and any component may have overrides.
#[derive(Debug, Clone, PartialEq)]
pub struct PostgresConfig {
    /// URL for the main (sequencer) database.
    pub master_url: Option<String>,
    /// URL for the replica database.
    pub replica_url: Option<String>,
    /// URL for the prover database.
    pub prover_url: Option<String>,
    /// Maximum size of the connection pool.
    pub max_connections: Option<u32>,
    /// Maximum size of the connection pool to master DB.
    pub max_connections_master: Option<u32>,

    /// Acquire timeout in seconds for a single connection attempt. There are multiple attempts (currently 3)
    /// before acquire methods will return an error.
    pub acquire_timeout_sec: Option<u64>,
    /// Statement timeout in seconds for Postgres connections. Applies only to the replica
    /// connection pool used by the API servers.
    pub statement_timeout_sec: Option<u64>,
    /// Threshold in milliseconds for the DB connection lifetime to denote it as long-living and log its details.
    pub long_connection_threshold_ms: Option<u64>,
    /// Threshold in milliseconds to denote a DB query as "slow" and log its details.
    pub slow_query_threshold_ms: Option<u64>,
}

impl PostgresConfig {
    /// Returns a copy of the master database URL as a `Result` to simplify error propagation.
    pub fn master_url(&self) -> anyhow::Result<&str> {
        self.master_url
            .as_deref()
            .context("Master DB URL is absent")
    }

    /// Returns a copy of the replica database URL as a `Result` to simplify error propagation.
    pub fn replica_url(&self) -> anyhow::Result<&str> {
        self.replica_url
            .as_deref()
            .context("Replica DB URL is absent")
    }

    /// Returns a copy of the prover database URL as a `Result` to simplify error propagation.
    pub fn prover_url(&self) -> anyhow::Result<&str> {
        self.prover_url
            .as_deref()
            .context("Prover DB URL is absent")
    }

    /// Returns the maximum size of the connection pool as a `Result` to simplify error propagation.
    pub fn max_connections(&self) -> anyhow::Result<u32> {
        self.max_connections.context("Max connections is absent")
    }

    pub fn max_connections_master(&self) -> Option<u32> {
        self.max_connections_master
    }

    /// Returns the Postgres statement timeout.
    pub fn statement_timeout(&self) -> Option<Duration> {
        self.statement_timeout_sec.map(Duration::from_secs)
    }

    /// Returns the acquire timeout for a single connection attempt.
    pub fn acquire_timeout(&self) -> Option<Duration> {
        self.acquire_timeout_sec.map(Duration::from_secs)
    }

    pub fn long_connection_threshold(&self) -> Option<Duration> {
        self.long_connection_threshold_ms.map(Duration::from_millis)
    }

    pub fn slow_query_threshold(&self) -> Option<Duration> {
        self.slow_query_threshold_ms.map(Duration::from_millis)
    }
}
