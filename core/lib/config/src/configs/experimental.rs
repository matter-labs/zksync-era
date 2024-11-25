//! Experimental part of configuration.

use std::{num::NonZeroU32, time::Duration};

use serde::Deserialize;
use smart_config::{
    metadata::{SizeUnit, TimeUnit},
    ByteSize, DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::{vm::FastVmMode, L1BatchNumber};

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ExperimentalDBConfig {
    /// Block cache capacity of the state keeper RocksDB cache. The default value is 128 MB.
    #[config(default_t = ByteSize::new(128, SizeUnit::MiB), with = SizeUnit::MiB)]
    pub state_keeper_db_block_cache_capacity_mb: ByteSize,
    /// Maximum number of files concurrently opened by state keeper cache RocksDB. Useful to fit into OS limits; can be used
    /// as a rudimentary way to control RAM usage of the cache.
    pub state_keeper_db_max_open_files: Option<NonZeroU32>,
    /// Configures whether to persist protective reads when persisting L1 batches in the state keeper.
    /// Protective reads are never required by full nodes so far, not until such a node runs a full Merkle tree
    /// (presumably, to participate in L1 batch proving).
    /// By default, set to `false` as it is expected that a separate `vm_runner_protective_reads` component
    /// which is capable of saving protective reads is run.
    #[config(default)]
    pub protective_reads_persistence_enabled: bool,
    // Merkle tree config
    /// Processing delay between processing L1 batches in the Merkle tree.
    #[config(default_t = Duration::from_millis(100), with = TimeUnit::Millis)]
    pub processing_delay_ms: Duration,
    /// If specified, RocksDB indices and Bloom filters will be managed by the block cache, rather than
    /// being loaded entirely into RAM on the RocksDB initialization. The block cache capacity should be increased
    /// correspondingly; otherwise, RocksDB performance can significantly degrade.
    #[config(default)]
    pub include_indices_and_filters_in_block_cache: bool,
    /// Enables the stale keys repair task for the Merkle tree.
    #[config(default)]
    pub merkle_tree_repair_stale_keys: bool,
}

/// Configuration for the VM playground (an experimental component that's unlikely to ever be stabilized).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ExperimentalVmPlaygroundConfig {
    /// Mode in which to run the fast VM implementation. Note that for it to actually be used, L1 batches should have a recent version.
    #[serde(default)]
    pub fast_vm_mode: FastVmMode,
    /// Path to the RocksDB cache directory.
    pub db_path: Option<String>,
    /// First L1 batch to consider processed. Will not be used if the processing cursor is persisted, unless the `reset` flag is set.
    #[serde(default)]
    pub first_processed_batch: L1BatchNumber,
    /// Maximum number of L1 batches to process in parallel.
    #[serde(default = "ExperimentalVmPlaygroundConfig::default_window_size")]
    pub window_size: NonZeroU32,
    /// If set to true, processing cursor will reset `first_processed_batch` regardless of the current progress. Beware that this will likely
    /// require to drop the RocksDB cache.
    #[serde(default)]
    pub reset: bool,
}

impl Default for ExperimentalVmPlaygroundConfig {
    fn default() -> Self {
        Self {
            fast_vm_mode: FastVmMode::default(),
            db_path: None,
            first_processed_batch: L1BatchNumber(0),
            window_size: Self::default_window_size(),
            reset: false,
        }
    }
}

impl ExperimentalVmPlaygroundConfig {
    pub fn default_window_size() -> NonZeroU32 {
        NonZeroU32::new(1).unwrap()
    }
}

/// Experimental VM configuration options.
#[derive(Debug, Clone, Default, PartialEq, Deserialize)]
pub struct ExperimentalVmConfig {
    #[serde(skip)] // Isn't properly deserialized by `envy`
    pub playground: ExperimentalVmPlaygroundConfig,

    /// Mode in which to run the fast VM implementation in the state keeper. Should not be set in production;
    /// the new VM doesn't produce call traces and can diverge from the old VM!
    #[serde(default)]
    pub state_keeper_fast_vm_mode: FastVmMode,

    /// Fast VM mode to use in the API server. Currently, some operations are not supported by the fast VM (e.g., `debug_traceCall`
    /// or transaction validation), so the legacy VM will always be used for them.
    #[serde(default)]
    pub api_fast_vm_mode: FastVmMode,
}
