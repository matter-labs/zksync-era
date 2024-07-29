//! Experimental part of configuration.

use std::num::NonZeroU32;

use serde::Deserialize;
use zksync_basic_types::vm::FastVmMode;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExperimentalDBConfig {
    /// Block cache capacity of the state keeper RocksDB cache. The default value is 128 MB.
    #[serde(default = "ExperimentalDBConfig::default_state_keeper_db_block_cache_capacity_mb")]
    pub state_keeper_db_block_cache_capacity_mb: usize,
    /// Maximum number of files concurrently opened by state keeper cache RocksDB. Useful to fit into OS limits; can be used
    /// as a rudimentary way to control RAM usage of the cache.
    pub state_keeper_db_max_open_files: Option<NonZeroU32>,
    /// Configures whether to persist protective reads when persisting L1 batches in the state keeper.
    /// Protective reads are never required by full nodes so far, not until such a node runs a full Merkle tree
    /// (presumably, to participate in L1 batch proving).
    /// By default, set to `true` as a temporary safety measure.
    #[serde(default = "ExperimentalDBConfig::default_protective_reads_persistence_enabled")]
    pub protective_reads_persistence_enabled: bool,
    // Merkle tree config
    /// Processing delay between processing L1 batches in the Merkle tree.
    #[serde(default = "ExperimentalDBConfig::default_merkle_tree_processing_delay_ms")]
    pub processing_delay_ms: u64,
    /// If specified, RocksDB indices and Bloom filters will be managed by the block cache, rather than
    /// being loaded entirely into RAM on the RocksDB initialization. The block cache capacity should be increased
    /// correspondingly; otherwise, RocksDB performance can significantly degrade.
    #[serde(default)]
    pub include_indices_and_filters_in_block_cache: bool,
}

impl Default for ExperimentalDBConfig {
    fn default() -> Self {
        Self {
            state_keeper_db_block_cache_capacity_mb:
                Self::default_state_keeper_db_block_cache_capacity_mb(),
            state_keeper_db_max_open_files: None,
            protective_reads_persistence_enabled:
                Self::default_protective_reads_persistence_enabled(),
            processing_delay_ms: Self::default_merkle_tree_processing_delay_ms(),
            include_indices_and_filters_in_block_cache: false,
        }
    }
}

impl ExperimentalDBConfig {
    const fn default_state_keeper_db_block_cache_capacity_mb() -> usize {
        128
    }

    pub fn state_keeper_db_block_cache_capacity(&self) -> usize {
        self.state_keeper_db_block_cache_capacity_mb * super::BYTES_IN_MEGABYTE
    }

    const fn default_protective_reads_persistence_enabled() -> bool {
        true
    }

    const fn default_merkle_tree_processing_delay_ms() -> u64 {
        100
    }
}

#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct ExperimentalVmConfig {
    /// Operation mode for the new fast VM.
    #[serde(default)]
    pub fast_vm_mode: FastVmMode,
}
