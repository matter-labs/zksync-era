use zksync_config::configs::da_client::DAClientConfig;

use crate::{envy_load, FromEnv};

impl FromEnv for DAClientConfig {
    fn from_env() -> anyhow::Result<Self> {
        envy_load("da_client", "DA_")
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::{
        configs::{
            da_client::{DAClient, DAClient::ObjectStore},
            object_store::ObjectStoreMode::GCS,
        },
        AvailConfig, DAClientConfig, ObjectStoreConfig,
    };

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_object_store_da_client_config(url: String, max_retries: u16) -> DAClientConfig {
        DAClientConfig {
            client: ObjectStore(ObjectStoreConfig {
                mode: GCS {
                    bucket_base_url: url,
                },
                max_retries,
                local_mirror_path: None,
            }),
        }
    }

    #[test]
    fn from_env_object_store() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="ObjectStore"

            DA_BUCKET_BASE_URL="sometestpath"
            DA_MODE="GCS"
            DA_MAX_RETRIES=5
        "#;
        lock.set_env(config);
        let actual = DAClientConfig::from_env().unwrap();
        assert_eq!(
            actual,
            expected_object_store_da_client_config("sometestpath".to_string(), 5)
        );
    }

    fn expected_avail_da_layer_config(
        api_node_url: &str,
        bridge_api_url: &str,
        seed: &str,
        app_id: u32,
        timeout: usize,
        max_retries: usize,
    ) -> DAClientConfig {
        DAClientConfig {
            client: DAClient::Avail(AvailConfig {
                api_node_url: api_node_url.to_string(),
                bridge_api_url: bridge_api_url.to_string(),
                seed: seed.to_string(),
                app_id,
                timeout,
                max_retries,
            }),
        }
    }

    #[test]
    fn from_env_avail_client() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="Avail"
            DA_API_NODE_URL="localhost:12345"
            DA_BRIDGE_API_URL="localhost:54321"
            DA_SEED="bottom drive obey lake curtain smoke basket hold race lonely fit walk"
            DA_APP_ID=1
            DA_TIMEOUT=2
            DA_MAX_RETRIES=3
        "#;

        lock.set_env(config);

        let actual = DAClientConfig::from_env().unwrap();
        assert_eq!(
            actual,
            expected_avail_da_layer_config(
                "localhost:12345",
                "localhost:54321",
                "bottom drive obey lake curtain smoke basket hold race lonely fit walk",
                "1".parse::<u32>().unwrap(),
                "2".parse::<usize>().unwrap(),
                "3".parse::<usize>().unwrap(),
            )
        );
    }
}
