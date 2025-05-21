use std::num::NonZeroUsize;

use smart_config::{
    de::{Optional, Serde},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::L1BatchNumber;

use crate::ObjectStoreConfig;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct TreeRecoveryConfig {
    /// Approximate chunk size (measured in the number of entries) to recover in a single iteration.
    /// Reasonable values are order of 100,000 (meaning an iteration takes several seconds).
    ///
    /// **Important.** This value cannot be changed in the middle of tree recovery (i.e., if a node is stopped in the middle
    /// of recovery and then restarted with a different config).
    #[config(default_t = 200_000)]
    pub chunk_size: u64,
    /// Buffer capacity for parallel persistence operations. Should be reasonably small since larger buffer means more RAM usage;
    /// buffer elements are persisted tree chunks. OTOH, small buffer can lead to persistence parallelization being inefficient.
    ///
    /// If not set, parallel persistence will be disabled.
    pub parallel_persistence_buffer: Option<NonZeroUsize>,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
#[config(derive(Default))]
pub struct PostgresRecoveryConfig {
    /// Maximum concurrency factor for the concurrent parts of snapshot recovery for Postgres. It may be useful to
    /// reduce this factor to about 5 if snapshot recovery overloads I/O capacity of the node. Conversely,
    /// if I/O capacity of your infra is high, you may increase concurrency to speed up Postgres recovery.
    #[config(default_t = NonZeroUsize::new(10).unwrap())]
    pub max_concurrency: NonZeroUsize,
}

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct SnapshotRecoveryConfig {
    /// Enables application-level snapshot recovery. Required to start a node that was recovered from a snapshot,
    /// or to initialize a node from a snapshot. Has no effect if a node that was initialized from a Postgres dump
    /// or was synced from genesis.
    ///
    /// This is an experimental and incomplete feature; do not use unless you know what you're doing.
    #[config(default)]
    pub enabled: bool,
    /// L1 batch number of the snapshot to use during recovery. Specifying this parameter is mostly useful for testing.
    #[config(with = Optional(Serde![int]))]
    pub l1_batch: Option<L1BatchNumber>,
    /// Enables dropping storage key preimages when recovering storage logs from a snapshot with version 0.
    /// This is a temporary flag that will eventually be removed together with version 0 snapshot support.
    #[config(default)]
    pub drop_storage_key_preimages: bool,
    #[config(nest)]
    pub tree: TreeRecoveryConfig,
    #[config(nest)]
    pub postgres: PostgresRecoveryConfig,
    #[config(nest)]
    // TODO: logically, this config is required if recovery is enabled, but this cannot be expressed yet
    pub object_store: Option<ObjectStoreConfig>,
}

#[cfg(test)]
mod tests {
    use smart_config::{ConfigRepository, ConfigSchema, Environment, Yaml};

    use super::*;
    use crate::configs::object_store::ObjectStoreMode;

    fn expected_config() -> SnapshotRecoveryConfig {
        SnapshotRecoveryConfig {
            enabled: false,
            l1_batch: Some(L1BatchNumber(1234)),
            drop_storage_key_preimages: true,
            tree: TreeRecoveryConfig {
                chunk_size: 250000,
                parallel_persistence_buffer: Some(NonZeroUsize::new(4).unwrap()),
            },
            postgres: PostgresRecoveryConfig {
                max_concurrency: NonZeroUsize::new(10).unwrap(),
            },
            object_store: Some(ObjectStoreConfig {
                mode: ObjectStoreMode::FileBacked {
                    file_backed_base_path: "./chains/era/artifacts/".into(),
                },
                max_retries: 100,
                local_mirror_path: None,
            }),
        }
    }

    fn create_schema() -> ConfigSchema {
        let mut schema = ConfigSchema::default();
        schema
            .insert(&SnapshotRecoveryConfig::DESCRIPTION, "snapshot_recovery")
            .unwrap()
            .push_alias("snapshots_recovery")
            .unwrap();
        schema
            .get_mut(
                &ObjectStoreConfig::DESCRIPTION,
                "snapshot_recovery.object_store",
            )
            .unwrap()
            .push_alias("snapshots.object_store")
            .unwrap();
        schema
    }

    #[test]
    fn parsing_from_env() {
        let env = r#"
            EN_SNAPSHOTS_RECOVERY_ENABLED=false
            EN_SNAPSHOTS_RECOVERY_L1_BATCH=1234
            EN_SNAPSHOTS_RECOVERY_DROP_STORAGE_KEY_PREIMAGES=true
            EN_SNAPSHOTS_RECOVERY_TREE_CHUNK_SIZE=250000
            EN_SNAPSHOTS_RECOVERY_TREE_PARALLEL_PERSISTENCE_BUFFER=4

            EN_SNAPSHOTS_OBJECT_STORE_MODE=FileBacked
            EN_SNAPSHOTS_OBJECT_STORE_MAX_RETRIES=100
            EN_SNAPSHOTS_OBJECT_STORE_FILE_BACKED_BASE_PATH=./chains/era/artifacts/
        "#;
        let env = Environment::from_dotenv("test.env", env)
            .unwrap()
            .strip_prefix("EN_");

        let schema = create_schema();
        let repo = ConfigRepository::new(&schema).with(env);
        let config: SnapshotRecoveryConfig = repo.single().unwrap().parse().unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
        snapshot_recovery:
          enabled: false
          l1_batch: 1234
          drop_storage_key_preimages: true
          postgres:
            max_concurrency: 10
          tree:
            chunk_size: 250000
            parallel_persistence_buffer: 4
          object_store:
            # file_backed:
            #   file_backed_base_path: ./chains/era/artifacts/
            mode: FileBacked
            file_backed_base_path: ./chains/era/artifacts/
            max_retries: 100
            local_mirror_path: null
          # experimental:
          #  drop_storage_key_preimages: true
          #  tree_recovery_parallel_persistence_buffer: 1
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let schema = create_schema();
        let repo = ConfigRepository::new(&schema).with(yaml);
        let config: SnapshotRecoveryConfig = repo.single().unwrap().parse().unwrap();
        assert_eq!(config, expected_config());
    }
}
