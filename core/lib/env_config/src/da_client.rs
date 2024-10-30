use std::env;

use zksync_config::configs::{
    da_client::{
        avail::AvailSecrets, DAClientConfig, AVAIL_CLIENT_CONFIG_NAME, EIGENDA_CLIENT_CONFIG_NAME,
        OBJECT_STORE_CLIENT_CONFIG_NAME,
    },
    secrets::DataAvailabilitySecrets,
};

use crate::{envy_load, FromEnv};

impl FromEnv for DAClientConfig {
    fn from_env() -> anyhow::Result<Self> {
        let client_tag = std::env::var("DA_CLIENT")?;
        let config = match client_tag.as_str() {
            AVAIL_CLIENT_CONFIG_NAME => Self::Avail(envy_load("da_avail_config", "DA_")?),
            OBJECT_STORE_CLIENT_CONFIG_NAME => {
                Self::ObjectStore(envy_load("da_object_store", "DA_")?)
            }
            EIGENDA_CLIENT_CONFIG_NAME => {
                DAClientConfig::EigenDA(envy_load("eigen_da_client", "EIGEN_DA_CLIENT_")?)
            }
            _ => anyhow::bail!("Unknown DA client name: {}", client_tag),
        };

        Ok(config)
    }
}

impl FromEnv for DataAvailabilitySecrets {
    fn from_env() -> anyhow::Result<Self> {
        let client_tag = std::env::var("DA_CLIENT")?;
        let secrets = match client_tag.as_str() {
            AVAIL_CLIENT_CONFIG_NAME => {
                let seed_phrase = env::var("DA_SECRETS_SEED_PHRASE")
                    .ok()
                    .map(|s| s.parse())
                    .transpose()?;
                Self::Avail(AvailSecrets { seed_phrase })
            }
            _ => anyhow::bail!("Unknown DA client name: {}", client_tag),
        };

        Ok(secrets)
    }
}

#[cfg(test)]
mod tests {
    use zksync_config::{
        configs::{
            da_client::{DAClientConfig, DAClientConfig::ObjectStore},
            object_store::ObjectStoreMode::GCS,
        },
        AvailConfig, ObjectStoreConfig,
    };

    use super::*;
    use crate::test_utils::EnvMutex;

    static MUTEX: EnvMutex = EnvMutex::new();

    fn expected_object_store_da_client_config(url: String, max_retries: u16) -> DAClientConfig {
        ObjectStore(ObjectStoreConfig {
            mode: GCS {
                bucket_base_url: url,
            },
            max_retries,
            local_mirror_path: None,
        })
    }

    #[test]
    fn from_env_object_store() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="ObjectStore"

            DA_BUCKET_BASE_URL="sometestpath"
            DA_MODE="GCS"
            DA_MAX_RETRIES="5"
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
        app_id: u32,
        timeout: usize,
        max_retries: usize,
    ) -> DAClientConfig {
        DAClientConfig::Avail(AvailConfig {
            api_node_url: api_node_url.to_string(),
            bridge_api_url: bridge_api_url.to_string(),
            app_id,
            timeout,
            max_retries,
        })
    }

    #[test]
    fn from_env_avail_client() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="Avail"
            DA_API_NODE_URL="localhost:12345"
            DA_BRIDGE_API_URL="localhost:54321"
            DA_APP_ID="1"
            DA_TIMEOUT="2"
            DA_MAX_RETRIES="3"
        "#;

        lock.set_env(config);

        let actual = DAClientConfig::from_env().unwrap();
        assert_eq!(
            actual,
            expected_avail_da_layer_config(
                "localhost:12345",
                "localhost:54321",
                "1".parse::<u32>().unwrap(),
                "2".parse::<usize>().unwrap(),
                "3".parse::<usize>().unwrap(),
            )
        );
    }

    #[test]
    fn from_env_avail_secrets() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="Avail"
            DA_SECRETS_SEED_PHRASE="bottom drive obey lake curtain smoke basket hold race lonely fit walk"
        "#;

        lock.set_env(config);

        let actual = match DataAvailabilitySecrets::from_env().unwrap() {
            DataAvailabilitySecrets::Avail(avail) => avail.seed_phrase,
        };

        assert_eq!(
            actual.unwrap(),
            "bottom drive obey lake curtain smoke basket hold race lonely fit walk"
                .parse()
                .unwrap()
        );
    }
}
