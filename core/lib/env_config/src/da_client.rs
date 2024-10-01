use std::env;

use zksync_config::configs::{
    da_client::{
        avail::AvailSecrets, celestia::CelestiaSecrets, DAClientConfig, AVAIL_CLIENT_CONFIG_NAME,
        CELESTIA_CLIENT_CONFIG_NAME, OBJECT_STORE_CLIENT_CONFIG_NAME,
    },
    secrets::DataAvailabilitySecrets,
};

use crate::{envy_load, FromEnv};

impl FromEnv for DAClientConfig {
    fn from_env() -> anyhow::Result<Self> {
        let client_tag = env::var("DA_CLIENT")?;
        let config = match client_tag.as_str() {
            AVAIL_CLIENT_CONFIG_NAME => Self::Avail(envy_load("da_avail_config", "DA_")?),
            CELESTIA_CLIENT_CONFIG_NAME => Self::Celestia(envy_load("da_celestia_config", "DA_")?),
            OBJECT_STORE_CLIENT_CONFIG_NAME => {
                Self::ObjectStore(envy_load("da_object_store", "DA_")?)
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
                    .map_err(|e| anyhow::format_err!("seed phrase not found: {}", e))?
                    .parse()
                    .map_err(|e| anyhow::format_err!("failed to parse the seed phrase: {}", e))?;

                Self::Avail(AvailSecrets { seed_phrase })
            }
            CELESTIA_CLIENT_CONFIG_NAME => {
                let auth_token = env::var("DA_SECRETS_AUTH_TOKEN")
                    .map_err(|e| anyhow::format_err!("auth token not found: {}", e))?
                    .parse()
                    .map_err(|e| anyhow::format_err!("failed to parse the auth token: {}", e))?;
                Self::Celestia(CelestiaSecrets { auth_token })
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
        AvailConfig, CelestiaConfig, ObjectStoreConfig,
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

            DA_SECRETS_SEED_PHRASE="bottom drive obey lake curtain smoke basket hold race lonely fit walk"
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

        let DataAvailabilitySecrets::Avail(actual) = DataAvailabilitySecrets::from_env().unwrap()
        else {
            panic!("expected Avail config")
        };
        assert_eq!(
            actual.seed_phrase,
            "bottom drive obey lake curtain smoke basket hold race lonely fit walk"
                .parse()
                .unwrap()
        );
    }

    fn expected_celestia_da_layer_config(api_node_url: &str, namespace: &str) -> DAClientConfig {
        DAClientConfig::Celestia(CelestiaConfig {
            api_node_url: api_node_url.to_string(),
            namespace: namespace.to_string(),
        })
    }

    #[test]
    fn from_env_celestia_client() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="Celestia"
            DA_API_NODE_URL="localhost:12345"
            DA_NAMESPACE="0x1234567890abcdef"
        "#;
        lock.set_env(config);

        let actual = DAClientConfig::from_env().unwrap();
        assert_eq!(
            actual,
            expected_celestia_da_layer_config("localhost:12345", "0x1234567890abcdef")
        );
    }

    #[test]
    fn from_env_celestia_secrets() {
        let mut lock = MUTEX.lock();
        let config = r#"
            DA_CLIENT="Celestia"
            DA_SECRETS_AUTH_TOKEN="0xf55baf7c0e4e33b1d78fbf52f069c426bc36cff1aceb9bc8f45d14c07f034d73"
        "#;

        lock.set_env(config);

        let DataAvailabilitySecrets::Celestia(actual) =
            DataAvailabilitySecrets::from_env().unwrap()
        else {
            panic!("expected Celestia config")
        };
        assert_eq!(
            actual.auth_token,
            "0xf55baf7c0e4e33b1d78fbf52f069c426bc36cff1aceb9bc8f45d14c07f034d73"
                .parse()
                .unwrap()
        );
    }
}
