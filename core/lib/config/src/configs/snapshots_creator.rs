use smart_config::{
    de::{Optional, Serde},
    DescribeConfig, DeserializeConfig,
};
use zksync_basic_types::L1BatchNumber;

use crate::ObjectStoreConfig;

#[derive(Debug, Clone, PartialEq, DescribeConfig, DeserializeConfig)]
pub struct SnapshotsCreatorConfig {
    /// Version of snapshots to create.
    // Raw integer version is used because `SnapshotVersion` is defined in `zksync_types` crate.
    #[config(default)]
    pub version: u16,
    /// L1 batch number to create the snapshot for. If not specified, a snapshot will be created
    /// for the current penultimate L1 batch.
    ///
    /// - If a snapshot with this L1 batch already exists and is complete, the creator will do nothing.
    /// - If a snapshot with this L1 batch exists and is incomplete, the creator will continue creating it,
    ///   regardless of whether the specified snapshot `version` matches.
    #[config(with = Optional(Serde![int]))]
    pub l1_batch_number: Option<L1BatchNumber>,
    #[config(default_t = 1_000_000)]
    pub storage_logs_chunk_size: u64,
    #[config(default_t = 25)]
    pub concurrent_queries_count: u32,
    #[config(nest)]
    pub object_store: ObjectStoreConfig,
}

#[cfg(test)]
mod tests {
    use smart_config::{ConfigRepository, ConfigSchema, Environment, Yaml};

    use super::*;
    use crate::configs::object_store::ObjectStoreMode;

    fn create_schema() -> ConfigSchema {
        let mut schema = ConfigSchema::default();
        schema
            .insert(&SnapshotsCreatorConfig::DESCRIPTION, "snapshot_creator")
            .unwrap()
            .push_alias("snapshots_creator")
            .unwrap();
        schema
            .get_mut(
                &ObjectStoreConfig::DESCRIPTION,
                "snapshot_creator.object_store",
            )
            .unwrap()
            .push_alias("snapshots.object_store")
            .unwrap();
        schema
    }

    fn expected_config() -> SnapshotsCreatorConfig {
        SnapshotsCreatorConfig {
            version: 0,
            l1_batch_number: Some(L1BatchNumber(1234)),
            storage_logs_chunk_size: 200000,
            concurrent_queries_count: 20,
            object_store: ObjectStoreConfig {
                mode: ObjectStoreMode::FileBacked {
                    file_backed_base_path: "./chains/era/artifacts/".into(),
                },
                max_retries: 100,
                local_mirror_path: None,
            },
        }
    }

    // Env config doesn't seem to be used anywhere, so this is an artificial test
    #[test]
    fn parsing_from_env() {
        let env = r#"
            SNAPSHOTS_CREATOR_STORAGE_LOGS_CHUNK_SIZE=200000
            SNAPSHOTS_CREATOR_CONCURRENT_QUERIES_COUNT=20
            SNAPSHOTS_CREATOR_VERSION=0
            SNAPSHOTS_CREATOR_L1_BATCH_NUMBER=1234

            SNAPSHOTS_OBJECT_STORE_MODE=FileBacked
            SNAPSHOTS_OBJECT_STORE_MAX_RETRIES=100
            SNAPSHOTS_OBJECT_STORE_FILE_BACKED_BASE_PATH=./chains/era/artifacts/
        "#;
        let env = Environment::from_dotenv("test.env", env).unwrap();

        let schema = create_schema();
        let repo = ConfigRepository::new(&schema).with(env);
        let config: SnapshotsCreatorConfig = repo.single().unwrap().parse().unwrap();
        assert_eq!(config, expected_config());
    }

    #[test]
    fn parsing_from_yaml() {
        let yaml = r#"
        snapshot_creator:
          storage_logs_chunk_size: 200000
          concurrent_queries_count: 20
          object_store:
            # file_backed:
            #   file_backed_base_path: ./chains/era/artifacts/
            mode: FileBacked
            file_backed_base_path: ./chains/era/artifacts/
            max_retries: 100
          version: 0
          l1_batch_number: 1234
        "#;
        let yaml = Yaml::new("test.yml", serde_yaml::from_str(yaml).unwrap()).unwrap();
        let schema = create_schema();
        let repo = ConfigRepository::new(&schema).with(yaml);
        let config: SnapshotsCreatorConfig = repo.single().unwrap().parse().unwrap();
        assert_eq!(config, expected_config());
    }
}
