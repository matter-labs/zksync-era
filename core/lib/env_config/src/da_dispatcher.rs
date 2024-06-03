use zksync_config::DADispatcherConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for DADispatcherConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("da_dispatcher", "DA_DISPATCHER_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::configs::{
        da_dispatcher::{DADispatcherConfig, DataAvailabilityMode},
        object_store::{ObjectStoreConfig, ObjectStoreMode},
    };
    use zksync_da_layers::{clients::celestia::config::CelestiaConfig, config::DALayerConfig};

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_gcs_config(
        bucket_base_url: &str,
        interval: u32,
        rows_limit: u32,
        max_retries: u16,
    ) -> DADispatcherConfig {
        DADispatcherConfig {
            da_mode: DataAvailabilityMode::ObjectStore(ObjectStoreConfig {
                mode: ObjectStoreMode::GCSWithCredentialFile {
                    bucket_base_url: bucket_base_url.to_owned(),
                    gcs_credential_file_path: "/path/to/credentials.json".to_owned(),
                },
                max_retries: 5,
            }),
            polling_interval_ms: Some(interval),
            query_rows_limit: Some(rows_limit),
            max_retries: Some(max_retries),
        }
    }

    fn expected_celestia_da_layer_config(
        pk: &str,
        interval: u32,
        rows_limit: u32,
        max_retries: u16,
    ) -> DADispatcherConfig {
        DADispatcherConfig {
            da_mode: DataAvailabilityMode::DALayer(DALayerConfig::Celestia(CelestiaConfig {
                light_node_url: "localhost:12345".to_string(),
                private_key: pk.to_owned(),
            })),
            polling_interval_ms: Some(interval),
            query_rows_limit: Some(rows_limit),
            max_retries: Some(max_retries),
        }
    }

    fn expected_no_da_config() -> DADispatcherConfig {
        DADispatcherConfig {
            da_mode: DataAvailabilityMode::NoDA,
            polling_interval_ms: None,
            query_rows_limit: None,
            max_retries: None,
        }
    }

    #[test]
    fn from_env_da_layer() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_DISPATCHER_POLLING_INTERVAL=5
            DA_DISPATCHER_QUERY_ROWS_LIMIT=60
            DA_DISPATCHER_MAX_RETRIES=7
            DA_DISPATCHER_DA_MODE="DALayer"
            DA_DISPATCHER_CLIENT_NAME="Celestia"
            DA_DISPATCHER_LIGHT_NODE_URL="localhost:12345"
            DA_DISPATCHER_PRIVATE_KEY="0xf55baf7c0e4e33b1d78fbf52f069c426bc36cff1aceb9bc8f45d14c07f034d73"
        "#;
        lock.set_env(config);
        let actual = DADispatcherConfig::from_env().unwrap();
        assert_eq!(
            actual,
            expected_celestia_da_layer_config(
                "0xf55baf7c0e4e33b1d78fbf52f069c426bc36cff1aceb9bc8f45d14c07f034d73",
                5,
                60,
                7
            )
        );
    }

    #[test]
    fn from_env_no_da() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_DISPATCHER_DA_MODE="NoDA"
        "#;
        lock.set_env(config);
        let actual = DADispatcherConfig::from_env().unwrap();
        assert_eq!(actual, expected_no_da_config());
    }

    #[test]
    fn from_env_object_store() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_DISPATCHER_POLLING_INTERVAL=10
            DA_DISPATCHER_QUERY_ROWS_LIMIT=100
            DA_DISPATCHER_MAX_RETRIES=6
            DA_DISPATCHER_DA_MODE="GCS"
            DA_DISPATCHER_MODE="GCSWithCredentialFile"
            DA_DISPATCHER_GCS_CREDENTIAL_FILE_PATH="/path/to/credentials.json"
            DA_DISPATCHER_BUCKET_BASE_URL="/base/url"
        "#;
        lock.set_env(config);
        let actual = DADispatcherConfig::from_env().unwrap();
        assert_eq!(actual, expected_gcs_config("/base/url", 10, 100, 6));
    }
}
