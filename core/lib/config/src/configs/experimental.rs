//! Experimental part of configuration.

use std::{num::NonZeroU32, path::PathBuf, time::Duration};

use smart_config::{de::Serde, metadata::SizeUnit, ByteSize, DescribeConfig, DeserializeConfig};
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
    // FIXME: Is this obsoleted by the state keeper param?
    #[config(default, alias = "reads_persistence_enabled")]
    pub protective_reads_persistence_enabled: bool,
    // Merkle tree config
    /// Processing delay between processing L1 batches in the Merkle tree.
    #[config(default_t = Duration::from_millis(100))]
    pub processing_delay: Duration,
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
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ExperimentalVmPlaygroundConfig {
    /// Mode in which to run the fast VM implementation. Note that for it to actually be used, L1 batches should have a recent version.
    #[config(default, with = Serde![str])]
    pub fast_vm_mode: FastVmMode,
    /// Path to the RocksDB cache directory.
    pub db_path: Option<PathBuf>,
    /// First L1 batch to consider processed. Will not be used if the processing cursor is persisted, unless the `reset` flag is set.
    #[config(default, with = Serde![int])]
    pub first_processed_batch: L1BatchNumber,
    /// Maximum number of L1 batches to process in parallel.
    #[config(default_t = NonZeroU32::new(1).unwrap())]
    pub window_size: NonZeroU32,
    /// If set to true, processing cursor will reset `first_processed_batch` regardless of the current progress. Beware that this will likely
    /// require to drop the RocksDB cache.
    #[config(default)]
    pub reset: bool,
}

/// Experimental VM configuration options.
#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct ExperimentalVmConfig {
    #[config(nest)]
    pub playground: ExperimentalVmPlaygroundConfig,
    /// Mode in which to run the fast VM implementation in the state keeper. Should not be set in production;
    /// the new VM doesn't produce call traces and can diverge from the old VM!
    #[config(default, with = Serde![str])]
    pub state_keeper_fast_vm_mode: FastVmMode,
    /// Fast VM mode to use in the API server. Currently, some operations are not supported by the fast VM (e.g., `debug_traceCall`
    /// or transaction validation), so the legacy VM will always be used for them.
    #[config(default, with = Serde![str])]
    pub api_fast_vm_mode: FastVmMode,
}

#[cfg(test)]
mod tests {
    use smart_config::{
        testing::{test_complete, Tester},
        Environment, Yaml,
    };

    use super::*;

    fn assert_experimental_vm_config(config: ExperimentalVmConfig) {
        assert_eq!(config.state_keeper_fast_vm_mode, FastVmMode::New);
        assert_eq!(config.api_fast_vm_mode, FastVmMode::Shadow);
        assert_eq!(config.playground.fast_vm_mode, FastVmMode::Shadow);
        assert_eq!(
            config.playground.db_path.unwrap().as_os_str(),
            "/db/vm_playground"
        );
        assert_eq!(config.playground.first_processed_batch, L1BatchNumber(123));
        assert_eq!(config.playground.window_size, NonZeroU32::new(1).unwrap());
        assert!(config.playground.reset);
    }

    #[test]
    fn experimental_vm_from_env() {
        let env = r#"
            EXPERIMENTAL_VM_STATE_KEEPER_FAST_VM_MODE=new
            EXPERIMENTAL_VM_API_FAST_VM_MODE=shadow
            EXPERIMENTAL_VM_PLAYGROUND_FAST_VM_MODE=shadow
            EXPERIMENTAL_VM_PLAYGROUND_DB_PATH=/db/vm_playground
            EXPERIMENTAL_VM_PLAYGROUND_FIRST_PROCESSED_BATCH=123
            EXPERIMENTAL_VM_PLAYGROUND_WINDOW_SIZE=1
            EXPERIMENTAL_VM_PLAYGROUND_RESET=true
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("EXPERIMENTAL_VM_");
        let config: ExperimentalVmConfig = test_complete(env).unwrap();
        assert_experimental_vm_config(config);
    }

    #[test]
    fn experimental_vm_from_yaml() {
        let yaml = r#"
          playground:
            fast_vm_mode: SHADOW
            db_path: /db/vm_playground
            first_processed_batch: 123
            reset: true
            window_size: 1
          state_keeper_fast_vm_mode: NEW
          api_fast_vm_mode: SHADOW
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let config: ExperimentalVmConfig = Tester::default()
            .coerce_variant_names()
            .test_complete(yaml)
            .unwrap();
        assert_experimental_vm_config(config);
    }
}
